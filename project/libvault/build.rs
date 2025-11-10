use std::{env, fs, path::Path};

use toml::Value;

// This is not going to happen any more since we have a default feature definition in Cargo.toml
//#[cfg(not(any(feature = "crypto_adaptor_openssl", feature = "crypto_adaptor_tongsuo")))]
//compile_error! {
//    r#"
//    No cryptography adaptor is enabled!
//
//    In RustyVault, the real cryptographic operations are done via "crypto_adaptor"s.
//
//    A crypto adaptor is a module that conveys and translates high level cryptography
//    operations like encryption, signing into the APIs provided by underlying cryptography
//    libraries such as OpenSSL, Tongsuo and so forth.
//
//    At current stage, only one crypto_adaptor can be enabled at compilation phase and later
//    be used at run-time. "crypto_adaptor"s are configured as 'feature's in the Cargo context.
//
//    Currently, the supported feature names of crypto adaptors are as follows, you can enable
//    them by adding one '--features crypto_adaptor_name' option when running "cargo build":
//        1. the OpenSSL adaptor: crypto_adaptor_openssl
//        2. the Tongsuo adaptor: crypto_adaptor_tongsuo
//    "#
//}

#[cfg(all(feature = "crypto_adaptor_openssl", feature = "crypto_adaptor_tongsuo"))]
compile_error! {
    r#"
    Only one cryptography adapator can be enabled!

    In RustyVault, the real cryptographic operations are done via "crypto_adaptor"s.

    A crypto adaptor is a module that conveys and translates high level cryptography
    operations like encryption, signing into the APIs provided by underlying cryptography
    libraries such as OpenSSL, Tongsuo and so forth.

    At current stage, only one crypto_adaptor can be enabled at compilation phase and later
    be used at run-time. "crypto_adaptor"s are configured as 'feature's in the Cargo context.

    Currently, the supported feature names of crypto adaptors are as follows, you can enable
    them by adding one '--features crypto_adaptor_name' option when running "cargo build":
        1. the OpenSSL adaptor: crypto_adaptor_openssl
        2. the Tongsuo adaptor: crypto_adaptor_tongsuo
    "#
}

fn main() {
    if env::var("DEP_OPENSSL_TONGSUO").is_ok() || cfg!(feature = "crypto_adaptor_tongsuo") {
        println!("cargo:rustc-cfg=tongsuo");
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let cargo_toml_path = Path::new(&manifest_dir).join("Cargo.toml");
    let content = match fs::read_to_string(cargo_toml_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let cargo_toml: Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    cargo_toml
        .get("bin")
        .and_then(|bin_table_value| bin_table_value.as_array())
        .map(|bin_table_array| {
            bin_table_array.iter().for_each(|bin_entry| {
                bin_entry
                    .as_table()
                    .and_then(|bin_entry_table| bin_entry_table.get("name"))
                    .and_then(|name_value| name_value.as_str())
                    .map(|name_str| println!("cargo:rustc-env=CARGO_BIN_NAME={name_str}"))
                    .unwrap_or(())
            })
        })
        .unwrap_or(());
}
