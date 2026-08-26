use std::{env, fs};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let [artifact_path, signature_path, public_key_path] = arguments.as_slice() else {
        return Err(
            "usage: verify-updater-artifact <artifact> <artifact.sig> <tauri-public-key-file>"
                .into(),
        );
    };

    let artifact = fs::read(artifact_path)?;
    let signature = decode_tauri_text(&fs::read_to_string(signature_path)?)?;
    let public_key = decode_tauri_text(&fs::read_to_string(public_key_path)?)?;
    PublicKey::decode(&public_key)?.verify(&artifact, &Signature::decode(&signature)?, true)?;
    println!("Updater signature is valid for {artifact_path}");
    Ok(())
}

fn decode_tauri_text(value: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(String::from_utf8(STANDARD.decode(value.trim())?)?)
}
