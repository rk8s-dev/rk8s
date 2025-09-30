---
sidebar_position: 5
title: Design
---
# RustyVault Design

As per: [RustyVault Requirements Document](./req.md). In this document we describe the architecture of RustyVault.

# Architecture Diagram

![RustyVault Architecture](../../static/img/RustyVault-arch.png)

Detailed description:

1. RustyVault contains three main components: RustyVault Core, RustyVault Modules and RustyVault Interface.
  * RustVault Core, the core component of RustyVault, contains many 'manager's. Each manager is in charge of a specific mechanism or layer. For instance, the 'Module Manager' handles all module management stuffs in RustyVault, providing mechanisms as module loading/unloading; meanwhile the 'Crypto Manager' provides one abstract layer for the crypto modules to call the underlying cryptography library to do the real cryptographic jobs.
  * RustVault Modules, which consists several modules, is where the real features of RustyVault take place. That is to say, most functionality code sits in RustyVault Modules. For instance, the PKI Module provides a whole CA functionality, such as issuing X.509 certificates; the Crypto Module then contains the code that invokes a specific cryptography library utilizing the abstrace layer provided by Crypto Manager in the RustyVault Core.
  * RustyVault Interface, is the part that interacts with the end users. The RustyVault Interface provides a set of RESTful APIs via an HTTPS server. After the server receives the API requests, it then routes these requests to one corresponding backend RustyVault Modules. That module then addresses the request and finally responds to the caller.

2. RustyVault depends on cryptography libraries (or cryptographic modules) to have the functionality such as encryption, signing, TLS connections and etc. Depending on the configuration, the cryptography library may be various, including native Rust crypto crates (ring, Rustls...), OpenSSL (via rust-openssl), Tongsuo...

3. RustyVault is able to utilize many different cryptographic hardware, such as HSMs or cryptography cards. But RustyVault doesn't talk to these hardware equipments directly, instead the underlying cryptography library sits between them and makes it easier for RustyVault to reach the hardware ability.

4. The sensitive data in RustyVault (such as secrets, credentials, password, keys...) can be stored in local storage or an external remote storage such as etcd. The external storage is necessary if RustyVault runs in cluster. Different storage method is managed by the Storage Manager in the RustyVault Core component. Thanks to this design, other modules of RustyVault don't need to deal with different types of storage.
