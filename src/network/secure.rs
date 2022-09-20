use std::error::Error;
use colorful::Colorful;
use colorful::Color;
use libp2p::identity;
use libp2p::identity::Keypair;
use libp2p::identity::ed25519::SecretKey;
use rand::rngs::OsRng;
use web3::signing::keccak256;

pub fn new_secret_key() -> Result<(), Box<dyn Error>> {
    let secret_key = secp256k1::SecretKey::new(&mut OsRng);
    println!("{}", "The secret_key only shows once!!! Please keep it safe.\n".color(Color::LightRed));
    println!("{}", secret_key.display_secret());
    Ok(())
}

pub fn generate_ed25519(key: &String) -> identity::Keypair {
    let mut hash = keccak256(key.as_bytes());
    let secret_key = SecretKey::from_bytes(&mut hash).unwrap();
    Keypair::Ed25519(secret_key.into())
}
