set fallback

[private]
default:
    @just --choose

run:
    mdbook serve --open

build: install
    mdbook build

install:
    cargo binstall -y mdbook-image-size --version 0.2.1
    cargo binstall -y mdbook-classy --version 0.1.0

test:
    mdbook test
