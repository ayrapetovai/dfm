use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce, Key
};

#[allow(unused_imports)]
use std::hash::Hasher;

#[allow(unused_imports)]
use rs_sha256::{HasherContext, Sha256Hasher};

// aes https://docs.rs/aes-gcm/latest/aes_gcm/
// sha https://lib.rs/crates/rs_sha256

#[allow(unused)]
fn encrypt(key_str: &[u8], plaintext: &str) -> Vec<u8> {
    let key = Key::<Aes256Gcm>::from_slice(key_str);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let cipher = Aes256Gcm::new(key);

    let ciphered_data = cipher.encrypt(&nonce, plaintext.as_bytes())
        .expect("failed to encrypt");

    // combining nonce and encrypted data together
    // for storage purpose
    let mut encrypted_data: Vec<u8> = nonce.to_vec();
    encrypted_data.extend_from_slice(&ciphered_data);

    encrypted_data
}

#[allow(unused)]
fn decrypt(key_str: &[u8], encrypted_data: &Vec<u8>) -> String {
    let key = Key::<Aes256Gcm>::from_slice(key_str);

    let (nonce_arr, ciphered_data) = encrypted_data.split_at(12);
    let nonce = Nonce::from_slice(nonce_arr);

    let cipher = Aes256Gcm::new(key);

    let plaintext = cipher.decrypt(nonce, ciphered_data)
        .expect("failed to decrypt data");

    // TODO rewrite to crate the string from raw bytes
    String::from_utf8(plaintext)
        .expect("failed to convert vector of bytes to string")
}

#[allow(unused)]
fn calc_password_hash(password: &str) -> [u8; 32] {
    let mut sha256hasher = Sha256Hasher::default();
    sha256hasher.write(password.as_bytes());

    let bytes_result = HasherContext::finish(&mut sha256hasher);
    return bytes_result.into();
}

#[test]
fn test_encryption() {
    let plaintext = "backendengineer.io";
    // let password = rpassword::read_password().unwrap();
    // let password = rpassword::prompt_password("password: ").unwrap();
    // let password = "Hello, encryptor! This password is really big and long";
    let password = "Hello";

    let key1 = calc_password_hash(&password);
    let encrypted_data = encrypt(&key1, plaintext);

    let key2 = calc_password_hash(&password);
    let decrypted_data = decrypt(&key2, &encrypted_data);

    println!("plain: {}", plaintext);
    println!("encrypted.size = {}", encrypted_data.len());
    println!("decry: {}", decrypted_data);

    assert_eq!(plaintext, decrypted_data);
}
