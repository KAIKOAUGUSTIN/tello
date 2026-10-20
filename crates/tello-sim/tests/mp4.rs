//! Gravação MP4 com o vídeo de teste, validada com o ffprobe quando disponível.

use std::process::Command;
use std::time::Duration;

use tello_core::mp4::Mp4Writer;
use tello_core::video::nal_types;

#[test]
fn records_fixture_to_playable_mp4() {
    let path = std::env::temp_dir().join(format!("tello-rec-{}.mp4", std::process::id()));
    let mut w = Mp4Writer::create(&path).unwrap();
    let frames = tello_sim::fixture_frames();
    // Começa num frame P: deve ser ignorado até o primeiro keyframe.
    w.write_frame(&frames[1], false, Duration::ZERO).unwrap();
    for (i, f) in frames.iter().enumerate() {
        let key = nal_types(f).contains(&5);
        w.write_frame(f, key, Duration::from_micros(i as u64 * 33_333)).unwrap();
    }
    let info = w.finish().unwrap();
    assert_eq!(info.frames, 90);
    assert!((info.duration.as_secs_f32() - 3.0).abs() < 0.05, "duração {:?}", info.duration);

    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[4..8], b"ftyp");
    assert!(bytes.windows(4).any(|w| w == b"avcC"));

    match Command::new("ffprobe")
        .args(["-v", "error", "-count_frames", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=codec_name,width,height,nb_read_frames,duration", "-of", "default=nw=1"])
        .arg(&path)
        .output()
    {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success(), "ffprobe falhou: {}", String::from_utf8_lossy(&out.stderr));
            assert!(text.contains("codec_name=h264"), "{text}");
            assert!(text.contains("width=960") && text.contains("height=720"), "{text}");
            assert!(text.contains("nb_read_frames=90"), "{text}");
        }
        Err(_) => eprintln!("ffprobe não encontrado: validação externa ignorada"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn bursty_frames_get_minimum_spacing() {
    let path = std::env::temp_dir().join(format!("tello-burst-{}.mp4", std::process::id()));
    let mut w = Mp4Writer::create(&path).unwrap();
    let frames = tello_sim::fixture_frames();
    // Dois frames por vez, a cada 66 ms (como numa rede com rajadas).
    for (i, f) in frames.iter().take(30).enumerate() {
        let key = nal_types(f).contains(&5);
        w.write_frame(f, key, Duration::from_millis((i as u64 / 2) * 66)).unwrap();
    }
    let info = w.finish().unwrap();
    assert_eq!(info.frames, 30);
    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts", "-of", "csv=p=0"])
        .arg(&path)
        .output();
    if let Ok(out) = probe {
        let pts: Vec<u64> =
            String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.trim().parse().ok()).collect();
        assert_eq!(pts.len(), 30);
        assert!(pts.windows(2).all(|w| w[1] >= w[0] + 1500), "espaçamento mínimo de 1/60 s: {pts:?}");
        // O tempo total segue o relógio real (~14 × 66 ms).
        assert!((*pts.last().unwrap() as f64 / 90_000.0 - 0.924).abs() < 0.05, "{pts:?}");
    }
    let _ = std::fs::remove_file(&path);
}
