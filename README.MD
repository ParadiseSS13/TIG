# TIG

A means of sending TCP over GRPC so it can be funneled through cloudflare, slightly tuned for BYOND.

## Important note

This only works well on stable connections. If you are playing on a mobile hotspot or heaven forbid satellite internet, this may be a very bad gameplay experience.

This is very much a proof of concept, not a viable solution.

## How to use with Paradise

1. Grab the latest release [from here](https://github.com/ParadiseSS13/TIG/releases/latest) depending on your system.
    - If you are unsure, get `tig-client.exe`, thats the windows one.
2. Open the respective `tig-client` executable. You should see the following output.

    ```txt
    Connected to server version 1.0.0
    Listening for connections on 127.0.0.1:6666
    ```

3. Paste `byond://localhost:6666` into run (`windows + r`). Make sure you have the BYOND pager/launcher running otherwise you wont be logged in!
4. When you first log in, your ping may be higher for 10 seconds or so. Wait a bit for this to stabilise then play.

## How to use with another server

The default config can be found [in this file](https://github.com/ParadiseSS13/TIG/blob/master/crates/client/config.example.yaml). Accepted config sources are:

- `config.{yml,yaml,toml,json,ini,ron,json5}`
- `config-client.{yml,yaml,toml,json,ini,ron,json5}`
- `TIG_CONFIG_OPTION_NAME` pointed to the file.

If you dont understand any of this, just use `config-client.yaml`.

## How do I verify the releases?

If you're a huge nerd, you can use the following provided you have GitHub CLI installed: `gh attestation verify -R ParadiseSS13/TIG tig-client.exe`.

## How to build

Windows:

1. Ensure you have rust installed.
2. Get a copy of [protoc](https://github.com/protocolbuffers/protobuf) and make sure its in your system path. You need this to build.
3. Run `cargo build --release` and get your file from `target/release`.

Linux:

*We advise the docker build, simply `docker build . --output .` to get the files. If you wish to build manually, please follow the steps below.*

1. Ensure you have rust installed.
2. Install the `protobuf-dev` package.
3. Run `cargo build --release` and get your file from `target/release`.

## Wheres the server

Server is currently closed source. We do plan to release it some day, just not sure when yet.

## License

This repository is licensed under the MIT license. You can find a copy in [LICENSE.MD](LICENSE.MD).
