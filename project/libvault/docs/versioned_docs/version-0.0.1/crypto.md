---
sidebar_position: 3
title: Crypto Adaptor
---
# RustyVault Crypto Adaptor

In RustyVault, we provide a mechanism for the users to build with selectable underlying cryptography libraries. This is the "crypto adaptor" mechanism.

Currently, only two adaptors are supported:

* OpenSSL crypto adaptor
* Tongsuo crypto adaptor

## The OpenSSL Crypto Adaptor

The following steps require a properly installed OpenSSL library. There are many ways of installing an OpenSSL on various platforms, so in this docuemnt we don't discuss that part.

The OpenSSL crypto adaptor is configured by default in RustyVault, so you can simply build RustyVault to enable it:

~~~
cargo build
~~~

Otherwise if you want to explicitly configure it, you can still use something like:

~~~
cargo build --features crypto_adaptor_openssl
~~~

But this is not necessary.

## The Tongsuo Crypto Adaptor

Tongsuo is a fork of OpenSSL aiming to have a better support on Chinese cryptography algorithms and standards. To use Tongsuo as the cryptography functionality provider in RustyVault, typically you need to build RustyVault as follows.

### Download and Install Tongsuo

Firstly, you need to have a copy of Tongsuo code and successfully build it into libraires and finally install it into somewhere in your machine.

Go to [https://tongsuo.net/docs/compilation/compile-and-install](https://tongsuo.net/docs/compilation/compile-and-install) for more detailed information.

### Configure RustyVault to use Tongsuo

RustyVault uses rust-tongsuo crate to call C APIs provided by Tongsuo. So we need to configure Cargo to use it, let's assume Tongsuo is successfully installed to `/path/to/tongsuo` directory: 

~~~
OPENSSL_DIR=/path/to/tongsuo cargo build \
  --features crypto_adaptor_tongsuo \
  --no-default-features \
  --config 'patch.crates-io.openssl.git="https://github.com/Tongsuo-Project/rust-tongsuo.git"' \
  --config 'patch.crates-io.openssl-sys.git="https://github.com/Tongsuo-Project/rust-tongsuo.git"'
~~~

Furthermore, if you choose to use a local copy of rust-tongsuo crate, you can use the file path form as well. Assume the local rust-tongsuo crate is located in `/path/to/rust-tongsuo` directory:

~~~
OPENSSL_DIR=/path/to/tongsuo cargo build \
  --features crypto_adaptor_tongsuo \
  --no-default-features \
  --config 'patch.crates-io.openssl.path="/path/to/rust-tongsuo/openssl"' \
  --config 'patch.crates-io.openssl-sys.path="/path/to/rust-tongsuo/openssl-sys"'
~~~

### The `LD_LIBRARY_PATH` Variable

If you are using Linux, then you may need to specify which path for RustyVault to look for the Tongsuo libraries. There are many ways of having this done, but in this document we demonstrate with the global environment variable way.

~~~
export LD_LIBRARY_PATH=/path/to/tongsuol/lib
~~~

Then you can run RustyVault smoothly.