#![feature(try_blocks)]

mod config;

use std::env;
use crate::config::TigConfig;
use bytes::{BufMut, BytesMut};
use color_eyre::Result;
use color_eyre::eyre::{ContextCompat, WrapErr};
use color_eyre::eyre::{bail, eyre};
use futures_util::FutureExt;
use std::io::ErrorKind;
use std::str::FromStr;
use std::time::Duration;
use tig_proto::proxy_client::ProxyClient;
use tig_proto::proxy_data::Data;
use tig_proto::version_client::VersionClient;
use tig_proto::{GetVersionRequest, ProxyData, ProxyDataEof, ProxyRequest};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::mpsc;
use tokio::{select, time};
use tokio_util::sync::CancellationToken;
use tonic::IntoRequest;
use tonic::codegen::tokio_stream::StreamExt;
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};

async fn connect(config: &TigConfig) -> Result<()> {
    let mut tls_config = ClientTlsConfig::new()
        .with_enabled_roots()
        .assume_http2(true);
    if config.use_ssl_key_log() {
        tls_config = tls_config.use_key_log()
    }

    let channel = Endpoint::from_str(&config.remote_address().to_string())
        .wrap_err("Invalid remote address")?
        .http2_keep_alive_interval(Duration::from_secs(30))
        .keep_alive_while_idle(true)
        .tls_config(tls_config)
        .wrap_err("Failed to load TLS configuration")?
        .connect()
        .await
        .wrap_err("Failed to establish channel")?;

    let proxy_client = ProxyClient::new(channel.clone());

    let token = CancellationToken::new();

    {
        let mut version_client = VersionClient::new(channel.clone());
        let server_version = version_client
            .get_version(GetVersionRequest {}.into_request())
            .await
            .wrap_err("Failed to request server version")?;

        let server_version = server_version
            .into_inner()
            .version
            .wrap_err("Server version empty")?;

        let server_version = semver::Version::parse(&server_version)
            .wrap_err(format!("Server version ({server_version}) invalid"))?;

        let local_version = tig_proto::protocol_version();

        if !semver::VersionReq::parse(&("^".to_owned() + local_version))?.matches(&server_version) {
            bail!(
                "Server version {server_version} is not compatible with local version {local_version}"
            );
        }

        println!("Connected to server version {server_version}");

        tokio::spawn({
            let token = token.clone();
            async move {
                let mut interval = time::interval(Duration::from_secs(10));
                loop {
                    token.run_until_cancelled(interval.tick()).await;
                    // Do not care about the result, just send requests to keep the connection alive
                    _ = token
                        .run_until_cancelled(
                            version_client.get_version(GetVersionRequest {}.into_request()),
                        )
                        .await;
                }
            }
        });
    }

    let socket = TcpSocket::new_v4().wrap_err("Failed to create tcpsocket")?;

    // Dont really care if we get told no
    _ = socket.set_nodelay(true);

    // Allow faster rebinding
    if cfg!(debug_assertions) {
        socket.set_reuseaddr(true)?;
    }

    socket
        .bind(config.listen_address())
        .wrap_err("Failed to bind tcpsocket")?;

    let listener = socket
        .listen(1024)
        .wrap_err("Failed to listen for connections")?;

    let local_address = listener.local_addr()?.ip();
    let local_port = listener.local_addr()?.port();

    println!("Listening for connections on {local_address}:{local_port}",);
    if let Some(open_url) = config.open_url().cloned() {
        let result = open::that_detached(
            open_url
                .replace("{ip}", &local_address.to_string())
                .replace("{port}", &local_port.to_string()),
        )
        .wrap_err(format!("Unable to open url {open_url}"));

        if let Err(err) = result {
            println!("{err:?}")
        }
    }

    loop {
        let connection = match token.run_until_cancelled(listener.accept()).await {
            Some(x) => x,
            None => break,
        }
        .wrap_err("Failed accepting new client")?;

        tokio::spawn({
            let connection_token = token.child_token();
            let proxy_client = proxy_client.clone();
            handle_connection(connection_token, proxy_client, connection.0, config.clone()).then(
                |result| async {
                    let result = result.wrap_err("A connection died because of an error");
                    if let Err(err) = &result {
                        println!("{err:?}")
                    }
                },
            )
        });
    }

    Ok(())
}

async fn handle_connection(
    connection_token: CancellationToken,
    mut proxy_client: ProxyClient<Channel>,
    connection: TcpStream,
    config: TigConfig,
) -> Result<()> {
    println!("Handling new connection");

    let result: Result<()> = try {
        let (mut local_receive, mut local_send) = connection.into_split();
        let (remote_write, remote_write_receiver) =
            mpsc::channel(config.remote_send_queue_length());

        println!("Connecting");
        let response = match connection_token
            .run_until_cancelled(
                proxy_client.proxy(ReceiverStream::new(remote_write_receiver).into_request()),
            )
            .await
        {
            Some(x) => x,
            None => return Ok(()),
        };

        let mut remote_read = response
            .wrap_err("Failed to connect to remote")?
            .into_inner();

        println!("Connection open");

        let termination_handler = || {
            {
                let connection_token = connection_token.clone();
                |result: Result<()>| async move {
                    let result = result.wrap_err("A connection died because of an error");

                    if let Err(err) = result {
                        // If either side errors out, we kill both because one side hung up both ends
                        connection_token.cancel();

                        println!("{err:?}",);
                    };
                }
            }
        };

        // Remote -> Local
        tokio::spawn(
            {
                let connection_token = connection_token.clone();
                let r2l_token = connection_token.child_token();
                async move {
                    let result: Result<()> = try {
                        loop {
                            let message = select! {
                                // Got a message
                                message = remote_read.next() => {
                                    match message {
                                        Some(x) => x,
                                        None => {
                                            // Remote closed both send and recv
                                            connection_token.cancel();
                                            break
                                        }
                                    }
                                },
                                // We're cancelled
                                _ = r2l_token.cancelled() => break,
                            };

                            let bytes = message
                                // Remote sent an error, killing the bidir connection, throw error
                                .wrap_err("Cannot read remote stream")?
                                .data
                                .and_then(|x| x.data);

                            let bytes = match bytes {
                                Some(x) => x,
                                // Not really supposed to be receiving empty messages, but we tolerate it
                                None => continue,
                            };

                            let bytes = match bytes {
                                // Remote sent explicit EOF, but could still be listening
                                Data::Eof(x) => match x.reason {
                                    Some(reason) => Err(eyre!(reason))?,
                                    None => break,
                                },
                                Data::Bytes(bytes) => bytes,
                            };

                            let result = select! {
                                // Wrote what we wanted to write
                                x = local_send.write_all(&bytes) => x,
                                // We're cancelled
                                _ = r2l_token.cancelled() => break,
                            };

                            // Local end is no longer receiving
                            if result
                                .as_ref()
                                .is_err_and(|x| x.kind() == ErrorKind::BrokenPipe)
                            {
                                break;
                            }

                            // Other type of error, throw an error and kill both sides
                            // Local could technically still write to us but this is weird so lets just reset
                            result.wrap_err("Failed to write to local stream")?;
                        }
                    };

                    let result = result.wrap_err("Remote disconnected due to an error");
                    if result.is_ok() {
                        println!("Remote disconnected");
                    }

                    // Loop has ended
                    r2l_token.cancel();
                    // Remote won't be writing anything anymore, drop the connection to local
                    drop(local_send);

                    result
                }
            }
            .then(termination_handler()),
        );

        // Local -> Remote
        tokio::spawn(
            {
                let l2r_token = connection_token.child_token();
                let remote_write = remote_write.clone();
                let mut buf = BytesMut::with_capacity(config.local_receive_buffer_size());
                async move {
                    let result: Result<()> = try {
                        loop {
                            let message = select! {
                                // Got a message
                                message = local_receive.read_buf(&mut buf) => message,
                                // We're cancelled
                                _ = l2r_token.cancelled() => break,
                            };

                            let read_size = message.wrap_err("Failed to read local stream")?;
                            // EOF is when we read 0 bytes but there's still space left in the buffer
                            // We stop sending to the client
                            if read_size == 0 && buf.remaining_mut() > 0 {
                                break;
                            }

                            remote_write
                                .send(ProxyRequest {
                                    data: Some(ProxyData {
                                        data: Some(Data::Bytes(buf.to_vec())),
                                    }),
                                })
                                .await
                                // This error kills both sides but that's ok because if we are here, it's because the remote completely hung up both sides
                                .wrap_err("Failed to write to remote stream")?;

                            buf.clear();
                        }
                    };

                    let result = result.wrap_err("Local disconnected due to an error");
                    if result.is_ok() {
                        println!("Local disconnected");
                    }

                    // Loop has ended
                    l2r_token.cancel();
                    // Local is no longer listening, send EOF to client
                    // If we cant send EOF to remote because they hung up, that's fine.
                    _ = remote_write
                        .send(ProxyRequest {
                            data: Some(ProxyData {
                                data: Some(Data::Eof(ProxyDataEof {
                                    // No special reason
                                    reason: result.as_ref().err().map(|err| format!("{err:#}")),
                                })),
                            }),
                        })
                        .await;
                    result
                }
            }
            .then(termination_handler()),
        );
    };
    result
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let config = TigConfig::new()?;

    if let Some(ssl_key_log) = config.ssl_key_log() {
        // This is safe because the only thing to access it is the grpc client
        // and the grpc client thread is not spawned until connect();
        unsafe {
            env::set_var("SSLKEYLOGFILE", ssl_key_log);
        }
    }

    connect(&config).await
}
