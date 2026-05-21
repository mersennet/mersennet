//! Noise Protocol encryption for Prime Chain P2P networking.
//! Uses Noise_XX_25519_ChaChaPoly_BLAKE2s (same pattern as libp2p, WireGuard).

use anyhow::{Result, bail};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use x25519_dalek::{PublicKey, StaticSecret};

const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";
const MAX_MESSAGE_LEN: usize = 65535;

/// X25519 keypair for Noise protocol.
#[allow(dead_code)]
pub struct NoiseKeypair {
    pub private_key: Vec<u8>,
    pub public_key: Vec<u8>,
}

impl NoiseKeypair {
    /// Generate a new X25519 keypair using snow.
    #[allow(dead_code)]
    pub fn generate() -> Self {
        let builder = snow::Builder::new(NOISE_PATTERN.parse().unwrap());
        let keypair = builder.generate_keypair().unwrap();
        Self {
            private_key: keypair.private,
            public_key: keypair.public,
        }
    }

    /// Reconstruct from existing private key (32 bytes).
    #[allow(dead_code)]
    pub fn from_bytes(private: &[u8]) -> Result<Self> {
        let bytes: [u8; 32] = private
            .try_into()
            .map_err(|_| anyhow::anyhow!("Private key must be 32 bytes"))?;
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret);
        Ok(Self {
            private_key: secret.to_bytes().to_vec(),
            public_key: public.to_bytes().to_vec(),
        })
    }
}

/// Wraps a Noise transport state for encrypted messaging.
#[allow(dead_code)]
pub struct NoiseSession {
    transport: snow::TransportState,
    remote_public_key: Vec<u8>,
}

impl NoiseSession {
    /// Encrypt a message.
    #[allow(dead_code)]
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        if plaintext.len() > MAX_MESSAGE_LEN {
            bail!("Message exceeds max length {}", MAX_MESSAGE_LEN);
        }
        let mut buf = vec![0u8; plaintext.len() + 16]; // ChaChaPoly overhead ~16 bytes
        let len = self.transport.write_message(plaintext, &mut buf)?;
        buf.truncate(len);
        Ok(buf)
    }

    /// Decrypt a message.
    #[allow(dead_code)]
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() > MAX_MESSAGE_LEN {
            bail!("Ciphertext exceeds max length {}", MAX_MESSAGE_LEN);
        }
        let mut buf = vec![0u8; ciphertext.len()];
        let len = self.transport.read_message(ciphertext, &mut buf)?;
        buf.truncate(len);
        Ok(buf)
    }

    /// Get the remote peer's public key.
    #[allow(dead_code)]
    pub fn remote_public_key(&self) -> &[u8] {
        &self.remote_public_key
    }
}

/// Manages encrypted connections with multiple peers.
#[allow(dead_code)]
pub struct NoiseTransport {
    keypair: NoiseKeypair,
    sessions: HashMap<SocketAddr, NoiseSession>,
}

impl NoiseTransport {
    #[allow(dead_code)]
    pub fn new(keypair: NoiseKeypair) -> Self {
        Self {
            keypair,
            sessions: HashMap::new(),
        }
    }

    /// Initiate Noise_XX handshake as initiator.
    #[allow(dead_code)]
    pub fn handshake_initiator(&mut self, peer: SocketAddr, stream: &mut TcpStream) -> Result<()> {
        let mut handshake = snow::Builder::new(NOISE_PATTERN.parse().unwrap())
            .local_private_key(&self.keypair.private_key)
            .build_initiator()
            .map_err(|e| anyhow::anyhow!("Build initiator: {:?}", e))?;

        let mut buf = [0u8; 1024];

        // Message 1: initiator -> responder (e)
        let len = handshake.write_message(&[], &mut buf)?;
        write_length_prefixed(stream, &buf[..len])?;

        // Message 2: responder -> initiator (e, ee, s, se)
        let len = read_length_prefixed(stream, &mut buf)?;
        handshake.read_message(&buf[..len], &mut [0u8; 1024])?;

        // Message 3: initiator -> responder (s, se)
        let len = handshake.write_message(&[], &mut buf)?;
        write_length_prefixed(stream, &buf[..len])?;

        let transport = handshake
            .into_transport_mode()
            .map_err(|e| anyhow::anyhow!("Into transport: {:?}", e))?;
        let remote_public_key = transport
            .get_remote_static()
            .map(|s: &[u8]| s.to_vec())
            .unwrap_or_default();

        self.sessions.insert(
            peer,
            NoiseSession {
                transport,
                remote_public_key,
            },
        );
        Ok(())
    }

    /// Accept Noise_XX handshake as responder.
    #[allow(dead_code)]
    pub fn handshake_responder(&mut self, peer: SocketAddr, stream: &mut TcpStream) -> Result<()> {
        let mut handshake = snow::Builder::new(NOISE_PATTERN.parse().unwrap())
            .local_private_key(&self.keypair.private_key)
            .build_responder()
            .map_err(|e| anyhow::anyhow!("Build responder: {:?}", e))?;

        let mut buf = [0u8; 1024];

        // Message 1: initiator -> responder (e)
        let len = read_length_prefixed(stream, &mut buf)?;
        handshake.read_message(&buf[..len], &mut [0u8; 1024])?;

        // Message 2: responder -> initiator (e, ee, s, se)
        let len = handshake.write_message(&[], &mut buf)?;
        write_length_prefixed(stream, &buf[..len])?;

        // Message 3: initiator -> responder (s, se)
        let len = read_length_prefixed(stream, &mut buf)?;
        handshake.read_message(&buf[..len], &mut [0u8; 1024])?;

        let transport = handshake
            .into_transport_mode()
            .map_err(|e| anyhow::anyhow!("Into transport: {:?}", e))?;
        let remote_public_key = transport
            .get_remote_static()
            .map(|s: &[u8]| s.to_vec())
            .unwrap_or_default();

        self.sessions.insert(
            peer,
            NoiseSession {
                transport,
                remote_public_key,
            },
        );
        Ok(())
    }

    /// Encrypt and send data with length-prefixed framing.
    #[allow(dead_code)]
    pub fn send_encrypted(
        &mut self,
        peer: &SocketAddr,
        data: &[u8],
        stream: &mut TcpStream,
    ) -> Result<()> {
        let session = self
            .sessions
            .get_mut(peer)
            .ok_or_else(|| anyhow::anyhow!("No session for peer {}", peer))?;
        let ciphertext = session.encrypt(data)?;
        write_length_prefixed(stream, &ciphertext)?;
        stream.flush()?;
        Ok(())
    }

    /// Receive and decrypt data with length-prefixed framing.
    #[allow(dead_code)]
    pub fn receive_encrypted(
        &mut self,
        peer: &SocketAddr,
        stream: &mut TcpStream,
    ) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; MAX_MESSAGE_LEN];
        let len = read_length_prefixed(stream, &mut buf)?;
        let session = self
            .sessions
            .get_mut(peer)
            .ok_or_else(|| anyhow::anyhow!("No session for peer {}", peer))?;
        session.decrypt(&buf[..len])
    }

    #[allow(dead_code)]
    pub fn is_connected(&self, peer: &SocketAddr) -> bool {
        self.sessions.contains_key(peer)
    }

    #[allow(dead_code)]
    pub fn disconnect(&mut self, peer: &SocketAddr) {
        self.sessions.remove(peer);
    }
}

/// Write length-prefixed framing: [4 bytes BE u32][payload]
fn write_length_prefixed(stream: &mut TcpStream, payload: &[u8]) -> Result<()> {
    let len = payload.len() as u32;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(payload)?;
    Ok(())
}

/// Read length-prefixed framing.
fn read_length_prefixed(stream: &mut TcpStream, buf: &mut [u8]) -> Result<usize> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > buf.len() {
        bail!("Message length {} exceeds buffer size {}", len, buf.len());
    }
    stream.read_exact(&mut buf[..len])?;
    Ok(len)
}

/// Server for accepting encrypted connections.
#[allow(dead_code)]
pub struct NoiseServer {
    transport: Arc<Mutex<NoiseTransport>>,
    listener: Option<TcpListener>,
}

impl NoiseServer {
    #[allow(dead_code)]
    pub fn new(keypair: NoiseKeypair) -> Self {
        Self {
            transport: Arc::new(Mutex::new(NoiseTransport::new(keypair))),
            listener: None,
        }
    }

    #[allow(dead_code)]
    pub fn bind(&mut self, addr: &str) -> Result<()> {
        let listener = TcpListener::bind(addr)?;
        self.listener = Some(listener);
        Ok(())
    }

    /// Accept a connection and perform responder handshake.
    #[allow(dead_code)]
    pub fn accept_connection(&self) -> Result<(SocketAddr, TcpStream)> {
        let listener = self
            .listener
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Server not bound"))?;
        let (mut stream, peer_addr) = listener.accept()?;
        let mut transport = self.transport.lock().unwrap();
        transport.handshake_responder(peer_addr, &mut stream)?;
        Ok((peer_addr, stream))
    }
}
