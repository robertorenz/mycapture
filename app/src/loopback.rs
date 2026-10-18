//! Desktop sound on Windows: WASAPI loopback of the default speakers, streamed to ffmpeg
//! as raw PCM over a local TCP socket. Silence is filled in so the track keeps pace with the video.
use std::io::Write;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use wasapi::{Direction, SampleType, StreamMode, WaveFormat};

use crate::record::{DESKTOP_CHANNELS as CHANNELS, DESKTOP_RATE as RATE};

const FRAME_BYTES: usize = 4 * CHANNELS;

pub struct Loopback {
    /// ffmpeg input for this stream (raw `f32le` PCM).
    pub url: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Loopback {
    /// Open the default output device for loopback and wait for ffmpeg to connect.
    pub fn start() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let url = format!("tcp://127.0.0.1:{}", listener.local_addr().map_err(|e| e.to_string())?.port());
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            if let Err(e) = run(listener, &flag, &ready_tx) {
                let _ = ready_tx.send(Err(e));
            }
        });
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self { url, stop, thread: Some(thread) }),
            Ok(Err(e)) => Err(format!("Desktop sound: {e}")),
            Err(_) => Err("Desktop sound: the audio device did not respond.".into()),
        }
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run(listener: TcpListener, stop: &AtomicBool, ready: &mpsc::Sender<Result<(), String>>) -> Result<(), String> {
    let e = |e: wasapi::WasapiError| e.to_string();
    let _ = wasapi::initialize_mta();
    let device = wasapi::DeviceEnumerator::new().map_err(e)?.get_default_device(&Direction::Render).map_err(e)?;
    let mut client = device.get_iaudioclient().map_err(e)?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, RATE, CHANNELS, None);
    let mode = StreamMode::PollingShared { autoconvert: true, buffer_duration_hns: 2_000_000 };
    client.initialize_client(&format, &Direction::Capture, &mode).map_err(e)?;
    let capture = client.get_audiocaptureclient().map_err(e)?;
    let _ = ready.send(Ok(()));

    // ffmpeg connects once it opens its inputs.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut sock = loop {
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(_) if stop.load(Ordering::Relaxed) || Instant::now() > deadline => return Ok(()),
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    let _ = sock.set_nonblocking(false);
    let _ = sock.set_nodelay(true);
    client.start_stream().map_err(e)?;
    let started = Instant::now();
    let mut written: usize = 0;
    let mut buf = vec![0u8; RATE * FRAME_BYTES];
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(10));
        while let Ok(Some(n)) = capture.get_next_packet_size() {
            if n == 0 {
                break;
            }
            let Ok((frames, info)) = capture.read_from_device(&mut buf) else { break };
            let bytes = &mut buf[..frames as usize * FRAME_BYTES];
            if info.flags.silent {
                bytes.fill(0);
            }
            if sock.write_all(bytes).is_err() {
                let _ = client.stop_stream();
                return Ok(());
            }
            written += frames as usize;
        }
        // Windows sends no packets while nothing plays: pad with silence to stay on the clock.
        let due = (started.elapsed().as_secs_f64() * RATE as f64) as usize;
        if due > written + RATE / 20 {
            let pad = (due - written).min(RATE);
            let zeros = vec![0u8; pad * FRAME_BYTES];
            if sock.write_all(&zeros).is_err() {
                break;
            }
            written += pad;
        }
    }
    let _ = client.stop_stream();
    Ok(())
}
