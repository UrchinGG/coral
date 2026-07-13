use std::io::{Cursor, Read};
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::debug;

use mc_proto::crypto::{Encryptor, ServerKey, server_hash};

use crate::codes::CodeStore;
use crate::protocol::*;
use crate::{FormatFn, auth};

const STATUS_NEXT_STATE: i32 = 1;
const LOGIN_NEXT_STATE: i32 = 2;
const VERIFY_TOKEN_LEN: usize = 4;
const PROTOCOL_1_20_5: i32 = 766;

pub struct ServerState {
    pub key: ServerKey,
    pub http: reqwest::Client,
    pub codes: CodeStore,
    pub motd: String,
    pub server_icon: Option<String>,
    pub format_disconnect: FormatFn,
}

pub async fn handle_connection(stream: TcpStream, state: Arc<ServerState>) {
    let addr = stream.peer_addr().ok();
    if let Err(e) = run_login_flow(stream, state).await {
        debug!("connection from {addr:?} ended: {e}");
    }
}

async fn run_login_flow(
    mut stream: TcpStream,
    state: Arc<ServerState>,
) -> Result<(), ConnectionError> {
    let (next_state, protocol) = read_handshake(&mut stream).await?;

    match next_state {
        STATUS_NEXT_STATE => return handle_status(&mut stream, &state, protocol).await,
        LOGIN_NEXT_STATE => {}
        _ => return Err(ConnectionError::InvalidNextState(next_state)),
    }

    let username = read_login_start(&mut stream).await?;
    let verify_token: [u8; VERIFY_TOKEN_LEN] = rand::random();
    send_encryption_request(&mut stream, &state.key, &verify_token, protocol).await?;

    let shared_secret = read_encryption_response(&mut stream, &state.key).await?;
    let mut cipher = Encryptor::new(&shared_secret);

    let hash = server_hash(b"", &shared_secret, &state.key.der_public_key);
    let player = auth::verify_session(&state.http, &username, &hash).await?;

    let code = state
        .codes
        .insert(player.uuid, player.username.clone())
        .await?;
    send_encrypted_disconnect(&mut stream, &mut cipher, &(state.format_disconnect)(&code)).await
}

async fn read_handshake(stream: &mut TcpStream) -> Result<(i32, i32), ConnectionError> {
    let (id, data) = read_packet(stream).await?;
    if id != 0x00 {
        return Err(ConnectionError::UnexpectedPacket(id));
    }
    let mut cursor = Cursor::new(data.as_slice());
    let protocol = read_varint_sync(&mut cursor)?;
    let _address = read_string(&mut cursor)?;
    let mut port = [0u8; 2];
    Read::read_exact(&mut cursor, &mut port)?;
    let next_state = read_varint_sync(&mut cursor)?;
    Ok((next_state, protocol))
}

async fn handle_status(
    stream: &mut TcpStream,
    state: &ServerState,
    protocol: i32,
) -> Result<(), ConnectionError> {
    let (id, _) = read_packet(stream).await?;
    if id != 0x00 {
        return Err(ConnectionError::UnexpectedPacket(id));
    }
    let mut payload = Vec::new();
    write_string(
        &mut payload,
        &build_status_json(&state.motd, state.server_icon.as_deref(), protocol),
    );
    write_packet(stream, 0x00, &payload).await?;
    if let Ok((0x01, ping_data)) = read_packet(stream).await {
        write_packet(stream, 0x01, &ping_data).await?;
    }
    Ok(())
}

fn build_status_json(motd: &str, icon: Option<&str>, protocol: i32) -> String {
    let mut resp = serde_json::json!({
        "version": { "name": "coral", "protocol": protocol },
        "players": { "max": 0, "online": 0 },
        "description": { "text": motd },
        "enforcesSecureChat": false,
    });
    if let Some(icon) = icon {
        resp["favicon"] = serde_json::Value::String(format!("data:image/png;base64,{icon}"));
    }
    resp.to_string()
}

async fn read_login_start(stream: &mut TcpStream) -> Result<String, ConnectionError> {
    let (id, data) = read_packet(stream).await?;
    if id != 0x00 {
        return Err(ConnectionError::UnexpectedPacket(id));
    }
    let mut cursor = Cursor::new(data.as_slice());
    Ok(read_string(&mut cursor)?)
}

async fn send_encryption_request(
    stream: &mut TcpStream,
    key: &ServerKey,
    verify_token: &[u8; VERIFY_TOKEN_LEN],
    protocol: i32,
) -> Result<(), ConnectionError> {
    let mut payload = Vec::new();
    write_string(&mut payload, "");
    write_varint(&mut payload, key.der_public_key.len() as i32);
    payload.extend_from_slice(&key.der_public_key);
    write_varint(&mut payload, VERIFY_TOKEN_LEN as i32);
    payload.extend_from_slice(verify_token);
    if protocol >= PROTOCOL_1_20_5 {
        payload.push(0x01);
    }
    write_packet(stream, 0x01, &payload).await?;
    Ok(())
}

async fn read_encryption_response(
    stream: &mut TcpStream,
    key: &ServerKey,
) -> Result<[u8; 16], ConnectionError> {
    let (id, data) = read_packet(stream).await?;
    if id != 0x01 {
        return Err(ConnectionError::UnexpectedPacket(id));
    }
    let mut cursor = Cursor::new(data.as_slice());
    let shared_secret = key
        .decrypt(&read_byte_array(&mut cursor)?)
        .map_err(|_| ConnectionError::DecryptionFailed)?;
    shared_secret
        .try_into()
        .map_err(|_| ConnectionError::InvalidSecretLength)
}

async fn send_encrypted_disconnect(
    stream: &mut TcpStream,
    cipher: &mut Encryptor,
    message: &str,
) -> Result<(), ConnectionError> {
    let mut payload = Vec::new();
    write_string(
        &mut payload,
        &serde_json::json!({ "text": message }).to_string(),
    );
    let mut packet = build_raw_packet(0x00, &payload);
    cipher.encrypt(&mut packet);
    stream.write_all(&packet).await?;
    stream.flush().await?;
    Ok(())
}

fn read_byte_array(cursor: &mut Cursor<&[u8]>) -> std::io::Result<Vec<u8>> {
    let len = read_varint_sync(cursor)? as usize;
    let mut buf = vec![0u8; len];
    Read::read_exact(cursor, &mut buf)?;
    Ok(buf)
}

fn build_raw_packet(packet_id: i32, payload: &[u8]) -> Vec<u8> {
    let id_len = varint_len(packet_id);
    let total_len = id_len + payload.len();
    let mut buf = Vec::with_capacity(varint_len(total_len as i32) + total_len);
    write_varint(&mut buf, total_len as i32);
    write_varint(&mut buf, packet_id);
    buf.extend_from_slice(payload);
    buf
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("auth: {0}")]
    Auth(#[from] auth::AuthError),
    #[error("unexpected packet id: {0}")]
    UnexpectedPacket(i32),
    #[error("invalid next state: {0}")]
    InvalidNextState(i32),
    #[error("RSA decryption failed")]
    DecryptionFailed,
    #[error("shared secret must be 16 bytes")]
    InvalidSecretLength,
    #[error("code generation: {0}")]
    CodeGeneration(#[from] anyhow::Error),
}
