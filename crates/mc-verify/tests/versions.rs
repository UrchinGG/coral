use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use mc_verify::VerifyServer;

const PORT: u16 = 25987;
const PROTOCOLS: &[i32] = &[47, 340, 578, 762, 766, 767, 770, 772, 774, 775, 776];
const PROTOCOL_1_20_5: i32 = 766;

fn write_varint(buf: &mut Vec<u8>, value: i32) {
    let mut val = value as u32;
    loop {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if val == 0 {
            break;
        }
    }
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    write_varint(buf, s.len() as i32);
    buf.extend_from_slice(s.as_bytes());
}

fn framed(id: i32, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    write_varint(&mut body, id);
    body.extend_from_slice(payload);
    let mut out = Vec::new();
    write_varint(&mut out, body.len() as i32);
    out.extend_from_slice(&body);
    out
}

fn read_varint(stream: &mut TcpStream) -> i32 {
    let mut value: i32 = 0;
    let mut pos = 0;
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).unwrap();
        value |= ((byte[0] & 0x7F) as i32) << pos;
        if byte[0] & 0x80 == 0 {
            return value;
        }
        pos += 7;
    }
}

fn read_packet(stream: &mut TcpStream) -> (i32, Vec<u8>) {
    let len = read_varint(stream) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).unwrap();
    let mut id_len = 0;
    let mut id = 0i32;
    let mut pos = 0;
    loop {
        let byte = buf[id_len];
        id |= ((byte & 0x7F) as i32) << pos;
        id_len += 1;
        if byte & 0x80 == 0 {
            break;
        }
        pos += 7;
    }
    (id, buf[id_len..].to_vec())
}

fn handshake(protocol: i32, next_state: i32) -> Vec<u8> {
    let mut p = Vec::new();
    write_varint(&mut p, protocol);
    write_string(&mut p, "localhost");
    p.extend_from_slice(&25565u16.to_be_bytes());
    write_varint(&mut p, next_state);
    p
}

fn connect() -> TcpStream {
    for _ in 0..50 {
        if let Ok(s) = TcpStream::connect(("127.0.0.1", PORT)) {
            return s;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("server never came up");
}

#[test]
fn login_flow_supports_all_versions() {
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            VerifyServer::new(
                format!("127.0.0.1:{PORT}"),
                "http://127.0.0.1:1",
                "test-key",
            )
            .start()
            .await
            .unwrap();
        });
    });

    let mut enc_request_len: Option<usize> = None;

    for &protocol in PROTOCOLS {
        let mut s = connect();
        s.write_all(&framed(0x00, &handshake(protocol, 1))).unwrap();
        s.write_all(&framed(0x00, &[])).unwrap();
        let (id, payload) = read_packet(&mut s);
        assert_eq!(id, 0x00, "status response id for protocol {protocol}");
        let json = String::from_utf8_lossy(&payload);
        assert!(
            json.contains(&format!("\"protocol\":{protocol}")),
            "status must echo protocol {protocol}, got: {json}"
        );

        let mut s = connect();
        s.write_all(&framed(0x00, &handshake(protocol, 2))).unwrap();
        let mut login_start = Vec::new();
        write_string(&mut login_start, "Tester");
        s.write_all(&framed(0x00, &login_start)).unwrap();
        let (id, payload) = read_packet(&mut s);
        assert_eq!(
            id, 0x01,
            "encryption request expected for protocol {protocol}"
        );

        if protocol < PROTOCOL_1_20_5 {
            enc_request_len = Some(payload.len());
        } else if let Some(base) = enc_request_len {
            assert_eq!(
                payload.len(),
                base + 1,
                "protocol {protocol} encryption request must include should-authenticate byte"
            );
        }
    }
}
