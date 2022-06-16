# Universal Inbox

[![Apache 2 License](https://img.shields.io/badge/license-Apache%202-blue.svg)](https://www.apache.org/licenses/)
[![Coverage Status](https://coveralls.io/repos/github/dax/universal-inbox/badge.svg?branch=main)](https://coveralls.io/github/dax/universal-inbox?branch=main)
[![CI](https://github.com/dax/universal-inbox/workflows/CI/badge.svg)](https://github.com/dax/universal-inbox/actions)

Universal Inbox is ...

## Features

- [ ] 
 
## Installation

### Using cargo (for development)

```bash
cargo make run
```

### Manual

1. Get the code

```bash
git clone https://github.com/dax/universal-inbox
```

2. Build api and web release assets

```bash
cargo make build-release
```

It will produce a `target/release/universal-inbox-api` backend binary and frontend assets in the `web/dist` directory.

3. Deploy assets

```bash
mkdir -p $DEPLOY_DIR/config
cp -a target/release/universal-inbox-api $DEPLOY_DIR
cp -a web/dist/* $DEPLOY_DIR
cp -a api/config/{default.toml, prod.toml} $DEPLOY_DIR/config
```

4. Run server

```bash
cd $DEPLOY_DIR
env CONFIG_FILE=$DEPLOY_DIR/config/prod.toml ./universal-inbox-api
```

### Using Docker

#### Build Docker image

```bash
docker build -t universal-inbox .
```

#### Run Universal Inbox using Docker

```bash
docker run --rm -ti -p 8000:8000 project-name
```

## Usage

Access Universal Inbox using [http://localhost:8000](http://localhost:8000)

## License

[Apache 2 License](LICENSE)
