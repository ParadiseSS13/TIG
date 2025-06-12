use color_eyre::eyre;
use color_eyre::eyre::WrapErr;
use config::Config;
use eyre::Result;
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Clone, Debug, Deserialize)]
pub struct TigConfig {
    listen_address: SocketAddr,
    remote_address: String,
    open_url: Option<String>,
    use_ssl_key_log: bool,
    ssl_key_log: Option<String>,
    remote_send_queue_length: usize,
    local_receive_buffer_size: usize,
}

#[allow(dead_code)]
impl TigConfig {
    pub fn listen_address(&self) -> SocketAddr {
        self.listen_address
    }
    pub fn remote_address(&self) -> &String {
        &self.remote_address
    }
    pub fn open_url(&self) -> Option<&String> {
        self.open_url.as_ref()
    }
    pub fn use_ssl_key_log(&self) -> bool {
        self.use_ssl_key_log && cfg!(debug_assertions)
    }
    pub fn ssl_key_log(&self) -> Option<&String> {
        if !cfg!(debug_assertions) {
            None
        } else {
            self.ssl_key_log.as_ref()
        }
    }
    pub fn remote_send_queue_length(&self) -> usize {
        self.remote_send_queue_length
    }
    pub fn local_receive_buffer_size(&self) -> usize {
        self.local_receive_buffer_size
    }
}

impl TigConfig {
    pub fn new() -> Result<Self> {
        let this = Config::builder()
            .add_source(::config::File::from_str(
                include_str!("default_config.yaml"),
                config::FileFormat::Yaml,
            ))
            .add_source(::config::File::with_name("config").required(false))
            .add_source(::config::File::with_name("config-client").required(false))
            .add_source(config::Environment::with_prefix("TIG"))
            .build()
            .wrap_err("Cannot build config")?
            .try_deserialize::<Self>()
            .wrap_err("Cannot deserialize config")?;

        Ok(this)
    }
}
