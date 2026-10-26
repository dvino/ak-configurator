//! Opt-in loopback OpenRGB SDK server; never owns or accesses HID.
//!
//! Serializer adapted to safe Rust from OpenRGB RGBController.cpp
//! GetDeviceDescriptionData/GetModeDescriptionData/GetZoneDescriptionData/
//! GetLEDDescriptionData, and NetworkProtocol.h (Adam Honse, 2019/2020).
//! Upstream SPDX-License-Identifier: GPL-2.0-or-later; adaptation distributed
//! under this application's GPL-3.0 license, exercising the later-version grant.
//! https://github.com/CalcProgrammer1/OpenRGB/tree/84d872e8ce1cdbcbd7ca17c7a68cfa1c536bfdde
//!
//! Only SDK 0/1 layouts are implemented (maximum negotiated version 1).
//! Device colors describe the latest client request, never hardware readback.
//! New code supplies bounds, latest-only handoff and stoppable socket ownership;
//! application verification and source arbitration remain with the caller.

use crate::ak820_rgb::{LED_COUNT, SLOT_MAP};
use crate::ak820_rgb_compositor::LED_POINTS;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(crate) const DEFAULT_PORT: u16 = 6742;
const PROTOCOL: u32 = 1;
const MAX_PAYLOAD: usize = 16 * 1024;
const PACKET_DEADLINE: Duration = Duration::from_secs(2);
pub(crate) type Frame = [[u8; 3]; LED_COUNT];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Listening,
    Connected,
    Disconnected,
    Stopped,
    Fault(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub status: Status,
    /// Unscaled source RGB (0..255); the sole HID scheduler applies its limit.
    pub frame: Option<Frame>,
}

pub(crate) struct Bridge {
    shared: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    port: u16,
}

impl Bridge {
    /// Caller must check the exact image/Direct capability before starting.
    /// A new explicit start is required after a client disconnect or fault.
    pub(crate) fn start(port: u16) -> io::Result<Self> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Mutex::new(Snapshot {
            status: Status::Listening,
            frame: None,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::clone(&shared);
        let flag = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("ak820-openrgb".into())
            .spawn(move || {
                let outcome = serve(listener, &state, &flag);
                let status = if flag.load(Ordering::Acquire) {
                    Status::Stopped
                } else {
                    match outcome {
                        Ok(()) => Status::Disconnected,
                        Err(e) => Status::Fault(e.to_string()),
                    }
                };
                publish(&state, status, None);
            })?;
        Ok(Self {
            shared,
            stop,
            worker: Some(worker),
            port,
        })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Takes at most one latest frame; no queue accumulates behind HID writes.
    pub(crate) fn poll(&self) -> Snapshot {
        let mut state = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        Snapshot {
            status: state.status.clone(),
            frame: state.frame.take(),
        }
    }

    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        publish(&self.shared, Status::Stopped, None);
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop();
    }
}

fn publish(shared: &Mutex<Snapshot>, status: Status, frame: Option<Frame>) {
    *shared.lock().unwrap_or_else(|e| e.into_inner()) = Snapshot { status, frame };
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn serve(listener: TcpListener, shared: &Mutex<Snapshot>, stop: &AtomicBool) -> io::Result<()> {
    let mut stream = loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        match listener.accept() {
            Ok((stream, peer)) if peer.ip().is_loopback() => break stream,
            Ok((stream, _)) => {
                let _ = stream.shutdown(Shutdown::Both);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(e) => return Err(e),
        }
    };
    // No second client can take over this session or queue behind it.
    drop(listener);
    stream.set_nonblocking(true)?;
    publish(shared, Status::Connected, None);
    let mut session = Session::default();
    let mut pending = Vec::with_capacity(MAX_PAYLOAD + 16);
    let mut packet_started = None;
    let mut scratch = [0; 2048];
    // A static-color client may remain cleanly idle indefinitely. Deadlines
    // apply only to incomplete packets and blocked responses, not inactivity.
    let result = (|| {
        while !stop.load(Ordering::Acquire) {
            match stream.read(&mut scratch) {
                Ok(0) => {
                    return if pending.is_empty() {
                        Ok(())
                    } else {
                        Err(invalid("truncated SDK packet"))
                    }
                }
                Ok(n) => {
                    if packet_started.is_none() {
                        packet_started = Some(Instant::now());
                    }
                    // Parse incrementally even when several packets arrive in one read.
                    for byte in &scratch[..n] {
                        pending.push(*byte);
                        if pending.len() >= 16 {
                            let (id, len) = header(&pending[..16])?;
                            if pending.len() == len + 16 {
                                let (response, changed) = session.handle(id, &pending[16..])?;
                                if let Some(payload) = response {
                                    write_packet(&mut stream, id, &payload, stop)?;
                                }
                                if changed {
                                    publish(shared, Status::Connected, Some(session.colors));
                                }
                                pending.clear();
                                packet_started = None;
                            }
                        }
                        if !pending.is_empty() && packet_started.is_none() {
                            packet_started = Some(Instant::now());
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => return Err(e),
            }
            if packet_started.is_some_and(|t| t.elapsed() > PACKET_DEADLINE) {
                return Err(invalid("SDK packet deadline exceeded"));
            }
        }
        Ok(())
    })();
    let _ = stream.shutdown(Shutdown::Both);
    result
}

fn header(bytes: &[u8]) -> io::Result<(u32, usize)> {
    if &bytes[..4] != b"ORGB" {
        return Err(invalid("invalid SDK magic"));
    }
    if u32_at(bytes, 4) != 0 {
        return Err(invalid("invalid SDK controller ID"));
    }
    let len = u32_at(bytes, 12) as usize;
    if len > MAX_PAYLOAD {
        return Err(invalid("SDK packet too large"));
    }
    Ok((u32_at(bytes, 8), len))
}

fn write_packet(
    stream: &mut TcpStream,
    id: u32,
    payload: &[u8],
    stop: &AtomicBool,
) -> io::Result<()> {
    let mut bytes = Vec::with_capacity(16 + payload.len());
    bytes.extend_from_slice(b"ORGB");
    put32(&mut bytes, 0);
    put32(&mut bytes, id);
    put32(&mut bytes, payload.len() as u32);
    bytes.extend_from_slice(payload);
    let start = Instant::now();
    let mut offset = 0;
    while offset < bytes.len() {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        if start.elapsed() > PACKET_DEADLINE {
            return Err(invalid("SDK response deadline exceeded"));
        }
        match stream.write(&bytes[offset..]) {
            Ok(0) => return Err(invalid("SDK response closed")),
            Ok(n) => offset += n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5))
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

struct Session {
    protocol: u32,
    colors: Frame,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            protocol: 0,
            colors: [[0; 3]; LED_COUNT],
        }
    }
}

impl Session {
    fn handle(&mut self, id: u32, data: &[u8]) -> io::Result<(Option<Vec<u8>>, bool)> {
        match id {
            40 if data.len() == 4 => {
                self.protocol = u32_at(data, 0).min(PROTOCOL);
                Ok((Some(PROTOCOL.to_le_bytes().to_vec()), false))
            }
            50 if !data.is_empty()
                && data.len() <= 128
                && data.last() == Some(&0)
                && !data[..data.len() - 1].contains(&0)
                && std::str::from_utf8(data).is_ok() =>
            {
                Ok((None, false))
            }
            0 if data.is_empty() => Ok((Some(1u32.to_le_bytes().to_vec()), false)),
            1 => {
                let version = if data.is_empty() {
                    0
                } else if data.len() == 4 {
                    u32_at(data, 0)
                } else {
                    return Err(invalid("invalid SDK controller request"));
                };
                if version > self.protocol {
                    return Err(invalid("unsupported SDK description version"));
                }
                Ok((Some(description(version, &self.colors)), false))
            }
            1100 if data.is_empty() => Ok((None, false)), // Only Direct exists.
            1101 => {
                // RGBController_Network::DeviceUpdateMode sends the full mode
                // description even when selecting the already active Direct.
                // SDK 0/1 have identical mode layouts. Accept only the exact
                // immutable mode advertised by this bridge; never save it.
                let mut mode = Vec::new();
                direct_mode(&mut mode);
                if data.len() != 8 + mode.len()
                    || u32_at(data, 0) as usize != data.len()
                    || u32_at(data, 4) != 0
                    || data[8..] != mode
                {
                    return Err(invalid("unsupported or malformed SDK Direct mode"));
                }
                Ok((None, false))
            }
            1050 | 1051 => {
                let offset = if id == 1051 { 8 } else { 4 };
                if data.len() < offset + 2
                    || u32_at(data, 0) as usize != data.len()
                    || (id == 1051 && u32_at(data, 4) != 0)
                    || u16_at(data, offset) as usize != LED_COUNT
                    || data.len() != offset + 2 + LED_COUNT * 4
                {
                    return Err(invalid("invalid SDK frame count, size or zone"));
                }
                let mut next = self.colors;
                for (i, color) in next.iter_mut().enumerate() {
                    let at = offset + 2 + i * 4;
                    if data[at + 3] != 0 {
                        return Err(invalid("invalid SDK color padding"));
                    }
                    color.copy_from_slice(&data[at..at + 3]);
                }
                self.colors = next;
                Ok((None, true))
            }
            1052 if data.len() == 8 => {
                let index = u32_at(data, 0) as usize;
                if index >= LED_COUNT || data[7] != 0 {
                    return Err(invalid("invalid SDK LED index or color"));
                }
                self.colors[index].copy_from_slice(&data[4..7]);
                Ok((None, true))
            }
            _ => Err(invalid("unsupported or malformed SDK operation")),
        }
    }
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn string(out: &mut Vec<u8>, value: &str) {
    put16(out, (value.len() + 1) as u16);
    out.extend_from_slice(value.as_bytes());
    out.push(0);
}

/// SDK 0/1 GetModeDescriptionData field sequence for immutable Direct.
fn direct_mode(out: &mut Vec<u8>) {
    string(out, "Direct");
    // value, flags (HAS_PER_LED_COLOR), speed min/max, colors min/max,
    // speed, direction, color mode (MODE_COLORS_PER_LED), color count.
    for value in [0, 1 << 5, 0, 0, 0, 0, 0, 0, 1] {
        put32(out, value);
    }
    put16(out, 0);
}

/// Safe bounded adaptation of upstream descriptor methods, SDK 0/1 only.
fn description(version: u32, colors: &Frame) -> Vec<u8> {
    let mut out = vec![0; 4]; // data_size includes itself (upstream GetDescription).
    put32(&mut out, 5); // DEVICE_TYPE_KEYBOARD
    string(&mut out, "AK820 (AK Configurator)");
    if version >= 1 {
        string(&mut out, "AJAZZ");
    }
    string(
        &mut out,
        "82 LEDs; requested host colors; no hardware readback",
    );
    string(&mut out, "ak820-beta2-slot-map-v1");
    string(&mut out, ""); // No device serial disclosure.
    string(&mut out, "AK Configurator loopback bridge");
    put16(&mut out, 1);
    put32(&mut out, 0); // one mode, active Direct
    direct_mode(&mut out);
    put16(&mut out, 1);
    string(&mut out, "Keyboard");
    put32(&mut out, 2); // ZONE_TYPE_MATRIX
    for _ in 0..3 {
        put32(&mut out, LED_COUNT as u32);
    }
    // A 17-column, six-row approximation uses floor(x / 14) for the
    // firmware physical centers. Exact centers remain in the compositor.
    // Electrical matrix columns and the unlit encoder press are not LEDs.
    const WIDTH: usize = 17;
    const ROW_Y: [u16; 6] = [0, 13, 26, 38, 51, 64];
    let mut matrix = [u32::MAX; 6 * WIDTH];
    for (i, &(x, y)) in LED_POINTS.iter().enumerate() {
        let row = ROW_Y
            .iter()
            .position(|v| *v == y)
            .expect("verified LED row");
        matrix[row * WIDTH + usize::from(x / 14)] = i as u32;
    }
    put16(&mut out, (8 + matrix.len() * 4) as u16);
    put32(&mut out, 6);
    put32(&mut out, WIDTH as u32);
    for index in matrix {
        put32(&mut out, index);
    }
    put16(&mut out, LED_COUNT as u16);
    for (i, slot) in SLOT_MAP.iter().enumerate() {
        string(&mut out, &format!("LED {}", i + 1));
        put32(&mut out, u32::from(*slot));
    }
    put16(&mut out, LED_COUNT as u16);
    for color in colors {
        out.extend_from_slice(color);
        out.push(0);
    }
    let size = out.len() as u32;
    out[..4].copy_from_slice(&size.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_payload(zone: bool) -> Vec<u8> {
        let mut data = vec![0; 4];
        if zone {
            put32(&mut data, 0);
        }
        put16(&mut data, LED_COUNT as u16);
        for i in 0..LED_COUNT {
            data.extend_from_slice(&[i as u8, 200, 255, 0]);
        }
        let len = data.len() as u32;
        data[..4].copy_from_slice(&len.to_le_bytes());
        data
    }

    #[test]
    fn rejects_unbounded_unknown_and_invalid_commands_without_mutating() {
        let mut s = Session::default();
        assert!(s.handle(40, &[0; 3]).is_err());
        assert!(s.handle(1, &2u32.to_le_bytes()).is_err());
        for id in [1000, 1102, 151, 152, 153, 200, u32::MAX] {
            assert!(s.handle(id, &[]).is_err());
        }
        let mut data = frame_payload(false);
        data[4..6].copy_from_slice(&83u16.to_le_bytes());
        assert!(s.handle(1050, &data).is_err());
        data = frame_payload(false);
        data[9] = 1;
        assert!(s.handle(1050, &data).is_err());
        data = frame_payload(true);
        data[4] = 1;
        assert!(s.handle(1051, &data).is_err());
        assert!(s.handle(1052, &[82, 0, 0, 0, 1, 2, 3, 0]).is_err());
        assert_eq!(s.colors, [[0; 3]; LED_COUNT]);
        let mut packet = b"ORGB".to_vec();
        put32(&mut packet, 0);
        put32(&mut packet, 0);
        put32(&mut packet, (MAX_PAYLOAD + 1) as u32);
        assert!(header(&packet).is_err());
        packet[12..16].copy_from_slice(&0u32.to_le_bytes());
        packet[4] = 1;
        assert!(header(&packet).is_err());
    }

    #[test]
    fn protocol_negotiates_one_and_updates_exactly_82_leds() {
        let mut s = Session::default();
        assert_eq!(
            s.handle(40, &6u32.to_le_bytes()).unwrap().0.unwrap(),
            1u32.to_le_bytes()
        );
        assert!(s.handle(1, &6u32.to_le_bytes()).is_err());
        assert!(s.handle(1100, &[]).is_ok());
        for (id, zone) in [(1050, false), (1051, true)] {
            assert!(s.handle(id, &frame_payload(zone)).unwrap().1);
            assert_eq!(s.colors[81], [81, 200, 255]);
        }
        s.handle(1052, &[81, 0, 0, 0, 7, 8, 9, 0]).unwrap();
        assert_eq!(s.colors[81], [7, 8, 9]);
    }

    #[test]
    fn accepts_upstream_shaped_direct_update_mode_and_rejects_changes() {
        // NetworkClient::SendRGBControllerUpdateMode uses u32 total size,
        // i32 mode index, then GetModeDescriptionData. Construct independently
        // from the bridge serializer to catch framing and field-order errors.
        let mut packet = 55u32.to_le_bytes().to_vec();
        packet.extend_from_slice(&0i32.to_le_bytes());
        packet.extend_from_slice(&7u16.to_le_bytes());
        packet.extend_from_slice(b"Direct\0");
        for field in [0u32, 32, 0, 0, 0, 0, 0, 0, 1] {
            packet.extend_from_slice(&field.to_le_bytes());
        }
        packet.extend_from_slice(&0u16.to_le_bytes());
        assert_eq!(packet.len(), 55);
        for version in [0u32, 1] {
            let mut session = Session::default();
            session.handle(40, &version.to_le_bytes()).unwrap();
            assert_eq!(session.handle(1101, &packet).unwrap(), (None, false));
            assert_eq!(session.colors, [[0; 3]; LED_COUNT]);
            assert!(session.handle(1102, &packet).is_err());
            for index in [0, 4, 8, 10, 17, 21, 25, 29, 33, 37, 41, 45, 49, 53] {
                let mut changed = packet.clone();
                changed[index] ^= 1;
                assert!(
                    session.handle(1101, &changed).is_err(),
                    "accepted changed byte {index}"
                );
            }
            assert!(session.handle(1101, &packet[..54]).is_err());
            assert!(session.handle(1101, &[]).is_err());
        }
    }

    // Independent reader using the published OpenRGB SDK 0/1 field sequence.
    // This is a host protocol test, not external-client or hardware evidence.
    struct Cursor<'a> {
        bytes: &'a [u8],
        at: usize,
    }
    impl Cursor<'_> {
        fn n16(&mut self) -> u16 {
            let v = u16_at(self.bytes, self.at);
            self.at += 2;
            v
        }
        fn n32(&mut self) -> u32 {
            let v = u32_at(self.bytes, self.at);
            self.at += 4;
            v
        }
        fn text(&mut self) -> &str {
            let len = usize::from(self.n16());
            let value = &self.bytes[self.at..self.at + len];
            self.at += len;
            assert_eq!(value[len - 1], 0);
            std::str::from_utf8(&value[..len - 1]).unwrap()
        }
    }

    #[test]
    fn descriptors_have_physical_matrix_and_no_83rd_led() {
        for version in [0, 1] {
            let bytes = description(version, &[[17, 18, 19]; LED_COUNT]);
            assert!(bytes.len() < MAX_PAYLOAD);
            let mut c = Cursor {
                bytes: &bytes,
                at: 0,
            };
            assert_eq!(c.n32() as usize, bytes.len());
            assert_eq!(c.n32(), 5);
            assert_eq!(c.text(), "AK820 (AK Configurator)");
            if version == 1 {
                assert_eq!(c.text(), "AJAZZ");
            }
            for _ in 0..4 {
                c.text();
            }
            assert_eq!(c.n16(), 1);
            assert_eq!(c.n32(), 0);
            assert_eq!(c.text(), "Direct");
            let fields: Vec<u32> = (0..9).map(|_| c.n32()).collect();
            assert_eq!(fields, [0, 32, 0, 0, 0, 0, 0, 0, 1]);
            assert_eq!(c.n16(), 0);
            assert_eq!(c.n16(), 1);
            assert_eq!(c.text(), "Keyboard");
            assert_eq!(c.n32(), 2);
            for _ in 0..3 {
                assert_eq!(c.n32(), 82);
            }
            assert_eq!(c.n16(), 416);
            assert_eq!(c.n32(), 6);
            assert_eq!(c.n32(), 17);
            let map: Vec<u32> = (0..6 * 17).map(|_| c.n32()).collect();
            let mut occupied: Vec<u32> = map.iter().copied().filter(|v| *v != u32::MAX).collect();
            occupied.sort_unstable();
            assert_eq!(occupied, (0..82).collect::<Vec<u32>>());
            assert_eq!(map[0], 0);
            assert_eq!(map[14], 13);
            assert_eq!(map[5 * 17 + 16], 81);
            assert_eq!(c.n16(), 82);
            for (i, slot) in SLOT_MAP.iter().enumerate() {
                assert_eq!(c.text(), format!("LED {}", i + 1));
                assert_eq!(c.n32(), u32::from(*slot));
            }
            assert_eq!(c.n16(), 82);
            for _ in 0..82 {
                assert_eq!(c.n32(), 0x00131211);
            }
            assert_eq!(c.at, bytes.len());
        }
    }

    fn send(stream: &mut TcpStream, id: u32, data: &[u8]) {
        let mut bytes = b"ORGB".to_vec();
        put32(&mut bytes, 0);
        put32(&mut bytes, id);
        put32(&mut bytes, data.len() as u32);
        bytes.extend_from_slice(data);
        stream.write_all(&bytes).unwrap();
    }
    fn receive(stream: &mut TcpStream, id: u32) -> Vec<u8> {
        let mut h = [0; 16];
        stream.read_exact(&mut h).unwrap();
        let (actual, size) = header(&h).unwrap();
        assert_eq!(actual, id);
        let mut data = vec![0; size];
        stream.read_exact(&mut data).unwrap();
        data
    }
    fn wait_status(bridge: &Bridge, expected: Status) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = bridge.poll();
            if snapshot.status == expected {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "status {:?}, wanted {:?}",
                snapshot.status,
                expected
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn loopback_session_handshake_latest_frame_disconnect_and_stop() {
        let mut bridge = Bridge::start(0).unwrap();
        assert_eq!(bridge.poll().status, Status::Listening);
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, bridge.port())).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        send(&mut client, 40, &6u32.to_le_bytes());
        assert_eq!(receive(&mut client, 40), 1u32.to_le_bytes());
        send(&mut client, 50, b"offline protocol test\0");
        send(&mut client, 0, &[]);
        assert_eq!(receive(&mut client, 0), 1u32.to_le_bytes());
        send(&mut client, 1, &1u32.to_le_bytes());
        assert_eq!(
            receive(&mut client, 1),
            description(1, &[[0; 3]; LED_COUNT])
        );
        send(&mut client, 1100, &[]);
        send(&mut client, 1050, &frame_payload(false));
        send(&mut client, 1052, &[81, 0, 0, 0, 7, 8, 9, 0]);
        // A following ordered response is a barrier for preceding updates.
        send(&mut client, 0, &[]);
        receive(&mut client, 0);
        let snapshot = bridge.poll();
        assert_eq!(snapshot.status, Status::Connected);
        let colors = snapshot.frame.unwrap();
        assert_eq!(colors[0], [0, 200, 255]);
        assert_eq!(colors[81], [7, 8, 9]);
        assert!(bridge.poll().frame.is_none());
        client.shutdown(Shutdown::Both).unwrap();
        assert!(wait_status(&bridge, Status::Disconnected).frame.is_none());
        bridge.stop();
        assert_eq!(bridge.poll().status, Status::Stopped);
        // Bound port is released even while the stopped object remains alive.
        let _listener = TcpListener::bind((Ipv4Addr::LOCALHOST, bridge.port())).unwrap();
    }

    #[test]
    fn stop_is_bounded_with_partial_packet() {
        let mut bridge = Bridge::start(0).unwrap();
        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, bridge.port())).unwrap();
        client.write_all(b"OR").unwrap();
        wait_status(&bridge, Status::Connected);
        let start = Instant::now();
        bridge.stop();
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(bridge.poll().frame.is_none());
    }
}
