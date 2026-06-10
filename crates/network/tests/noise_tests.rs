use mersennet_network::noise::NoiseKeypair;

#[test]
fn noise_keypair_generation() {
    let kp = NoiseKeypair::generate();
    assert_eq!(kp.private_key.len(), 32);
    assert_eq!(kp.public_key.len(), 32);
    assert_ne!(kp.private_key, kp.public_key);
}

#[test]
fn noise_keypair_from_bytes() {
    let kp1 = NoiseKeypair::generate();
    let kp2 = NoiseKeypair::from_bytes(&kp1.private_key).unwrap();
    assert_eq!(kp1.public_key, kp2.public_key);
}
