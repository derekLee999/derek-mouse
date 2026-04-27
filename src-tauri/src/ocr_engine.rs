use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::Mutex,
};

use base64::{engine::general_purpose, Engine as _};
use image::{
    codecs::png::{CompressionType, FilterType as PngFilterType, PngEncoder},
    imageops::{grayscale, resize, FilterType},
    ExtendedColorType, GrayImage, ImageEncoder,
};
use serde::{Deserialize, Serialize};


const OCR_DOWNLOAD_URL: &str = "https://github.com/hiroi-sora/RapidOCR-json/releases/download/v0.2.0/RapidOCR-json_v0.2.0.7z";
const OCR_SHA256: &str = "7ad9b283d03436c6cd0296723188699299cb4e5cf9140b410c59543aa5793c40";
const OCR_EXE_NAME: &str = "RapidOCR-json.exe";
const OCR_MAX_IMAGE_DIMENSION: u32 = 1920;
const OCR_MAX_IMAGE_PIXELS: u64 = 2_500_000;

pub fn engine_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".derek-mouse").join("ocr-engine"))
}

pub fn engine_exe_path() -> Option<PathBuf> {
    engine_dir().map(|dir| dir.join(OCR_EXE_NAME))
}

pub fn is_installed() -> bool {
    engine_exe_path().map(|p| p.exists()).unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub status: String,
    pub progress: u8,
    pub message: String,
}

pub async fn download_and_install<F>(progress_cb: F) -> Result<(), String>
where
    F: Fn(InstallProgress) + Send + Sync + 'static,
{
    let dir = engine_dir().ok_or_else(|| "Could not determine engine directory".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create engine directory: {e}"))?;

    let temp_file = tempfile::NamedTempFile::with_suffix_in(".7z", &dir)
        .map_err(|e| format!("Could not create temp file: {e}"))?;
    let temp_path = temp_file.path().to_path_buf();
    drop(temp_file);

    progress_cb(InstallProgress {
        status: "downloading".to_string(),
        progress: 0,
        message: "开始下载 OCR 引擎...".to_string(),
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(OCR_DOWNLOAD_URL)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    let total_size = response
        .content_length()
        .ok_or_else(|| "Could not determine download size".to_string())?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut file = std::fs::File::create(&temp_path)
        .map_err(|e| format!("Could not create download file: {e}"))?;

    use futures_util::StreamExt;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Download stream error: {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Could not write download chunk: {e}"))?;
        downloaded += chunk.len() as u64;
        let progress = ((downloaded as f64 / total_size as f64) * 100.0) as u8;
        progress_cb(InstallProgress {
            status: "downloading".to_string(),
            progress,
            message: format!("正在下载 OCR 引擎... {progress}%"),
        });
    }

    drop(file);

    progress_cb(InstallProgress {
        status: "verifying".to_string(),
        progress: 100,
        message: "下载完成，正在校验...".to_string(),
    });

    let file_bytes = std::fs::read(&temp_path)
        .map_err(|e| format!("Could not read downloaded file: {e}"))?;
    let hash = sha256::digest(&file_bytes);
    if hash != OCR_SHA256 {
        let _ = std::fs::remove_file(&temp_path);
        return Err("Downloaded file checksum mismatch".to_string());
    }

    progress_cb(InstallProgress {
        status: "extracting".to_string(),
        progress: 0,
        message: "正在解压 OCR 引擎...".to_string(),
    });

    let extract_dir = dir.join("extract_temp");
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir)
        .map_err(|e| format!("Could not create extract directory: {e}"))?;

    sevenz_rust::decompress_file(&temp_path, &extract_dir)
        .map_err(|e| format!("Could not extract archive: {e:?}"))?;

    let _ = std::fs::remove_file(&temp_path);

    // 递归查找包含 RapidOCR-json.exe 的目录
    let source_dir = find_dir_containing(&extract_dir, OCR_EXE_NAME)
        .ok_or_else(|| "Engine executable not found after extraction".to_string())?;

    // 先复制到目标目录，再删除临时目录（顺序不能反，extract_dir 是 dir 的子目录）
    copy_dir_all(&source_dir, &dir)?;
    let _ = std::fs::remove_dir_all(&extract_dir);

    if !engine_exe_path().map(|p| p.exists()).unwrap_or(false) {
        return Err("Engine executable not found after extraction".to_string());
    }

    progress_cb(InstallProgress {
        status: "completed".to_string(),
        progress: 100,
        message: "安装完成".to_string(),
    });

    Ok(())
}

fn find_dir_containing(dir: &Path, filename: &str) -> Option<PathBuf> {
    if dir.join(filename).exists() {
        return Some(dir.to_path_buf());
    }
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        if entry.file_type().ok()?.is_dir() {
            if let Some(found) = find_dir_containing(&entry.path(), filename) {
                return Some(found);
            }
        }
    }
    None
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), String> {
    std::fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            let dest = dst.as_ref().join(entry.file_name());
            let _ = std::fs::remove_file(&dest);
            std::fs::copy(entry.path(), dest)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrText {
    pub text: String,
    pub score: f64,
    pub center_x: i32,
    pub center_y: i32,
}

pub struct PreparedOcrImage {
    pub data_url: String,
    pub scale_x: f32,
    pub scale_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OcrResponse {
    code: i32,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OcrDataItem {
    text: String,
    score: f64,
    #[allow(dead_code)]
    #[serde(rename = "box")]
    bbox: Vec<Vec<i32>>,
}

pub struct OcrEngine {
    child: Child,
    stdin: Mutex<ChildStdin>,
    stdout_rx: Mutex<std::sync::mpsc::Receiver<String>>,
}

impl OcrEngine {
    pub fn new() -> Result<Self, String> {
        let exe = engine_exe_path()
            .ok_or_else(|| "Engine path not available".to_string())?;
        if !exe.exists() {
            return Err("OCR engine is not installed".to_string());
        }

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let work_dir = exe.parent().unwrap();
        let mut child = Command::new(&exe)
            .current_dir(work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Could not start OCR engine: {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Could not capture OCR stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Could not capture OCR stdout".to_string())?;

        // 启动后台线程持续读取 stdout，避免主线程阻塞
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut buf = String::new();
            loop {
                match reader.read_line(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => {
                        let line = buf.trim().to_string();
                        if !line.is_empty() {
                            let _ = tx.send(line);
                        }
                        buf.clear();
                    }
                    Err(_) => break,
                }
            }
        });

        // 等待引擎初始化（加载模型）
        std::thread::sleep(std::time::Duration::from_secs(5));

        Ok(OcrEngine {
            child,
            stdin: Mutex::new(stdin),
            stdout_rx: Mutex::new(rx),
        })
    }

    pub fn recognize(&self, image_data_url: &str) -> Result<Vec<OcrText>, String> {
        let encoded = image_data_url
            .split_once(',')
            .map(|(_, data)| data)
            .unwrap_or(image_data_url);

        let json_input = format!("{{\"image_base64\":\"{encoded}\"}}\n");

        {
            let mut stdin = self.stdin.lock().map_err(|e| e.to_string())?;
            stdin
                .write_all(json_input.as_bytes())
                .map_err(|e| format!("Could not send data to OCR engine: {e}"))?;
            stdin
                .flush()
                .map_err(|e| format!("Could not flush OCR stdin: {e}"))?;
        }

        let rx = self.stdout_rx.lock().map_err(|e| e.to_string())?;
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if std::time::Instant::now() > timeout {
                return Err("OCR recognition timeout".to_string());
            }
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(line) => {
                    if line.is_empty() {
                        continue;
                    }
                    // 跳过非 JSON 的初始化输出行（如版本信息）
                    let response: OcrResponse = match serde_json::from_str(&line) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                    if response.code == 101 {
                        // 未找到文字
                        return Ok(Vec::new());
                    }
                    if response.code != 100 {
                        return Err(format!(
                            "OCR engine returned error code: {}. Raw: {line}",
                            response.code
                        ));
                    }

                    let items: Vec<OcrDataItem> = match response.data {
                        serde_json::Value::Array(arr) => arr
                            .into_iter()
                            .filter_map(|v| serde_json::from_value(v).ok())
                            .collect(),
                        _ => Vec::new(),
                    };

                    let results: Vec<OcrText> = items
                        .into_iter()
                        .map(|item| {
                            let center_x = if item.bbox.len() >= 4 {
                                let xs: Vec<i32> = item.bbox.iter().map(|p| p[0]).collect();
                                let min_x = xs.iter().copied().min().unwrap_or(0);
                                let max_x = xs.iter().copied().max().unwrap_or(0);
                                (min_x + max_x) / 2
                            } else {
                                0
                            };
                            let center_y = if item.bbox.len() >= 4 {
                                let ys: Vec<i32> = item.bbox.iter().map(|p| p[1]).collect();
                                let min_y = ys.iter().copied().min().unwrap_or(0);
                                let max_y = ys.iter().copied().max().unwrap_or(0);
                                (min_y + max_y) / 2
                            } else {
                                0
                            };
                            OcrText {
                                text: item.text,
                                score: item.score,
                                center_x,
                                center_y,
                            }
                        })
                        .collect();

                    return Ok(results);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => return Err("OCR channel closed".to_string()),
            }
        }
    }
}

impl Drop for OcrEngine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn prepare_image_for_ocr(image: &image::RgbaImage) -> Result<PreparedOcrImage, String> {
    let original_width = image.width();
    let original_height = image.height();
    let grayscale = grayscale(image);
    let processed = downscale_for_ocr(grayscale);
    let encoded = encode_gray_png(&processed)?;

    Ok(PreparedOcrImage {
        data_url: format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(encoded)),
        scale_x: original_width as f32 / processed.width() as f32,
        scale_y: original_height as f32 / processed.height() as f32,
    })
}

fn downscale_for_ocr(image: GrayImage) -> GrayImage {
    let width = image.width();
    let height = image.height();
    let pixel_count = u64::from(width) * u64::from(height);
    let max_dimension = width.max(height);

    if max_dimension <= OCR_MAX_IMAGE_DIMENSION && pixel_count <= OCR_MAX_IMAGE_PIXELS {
        return image;
    }

    let scale = OCR_MAX_IMAGE_DIMENSION as f32 / max_dimension as f32;
    let target_width = ((width as f32 * scale).round() as u32).max(1);
    let target_height = ((height as f32 * scale).round() as u32).max(1);
    resize(&image, target_width, target_height, FilterType::Triangle)
}

fn encode_gray_png(image: &GrayImage) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    PngEncoder::new_with_quality(&mut buffer, CompressionType::Fast, PngFilterType::NoFilter)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::L8,
        )
        .map_err(|e| e.to_string())?;
    Ok(buffer)
}



// 简单的 SHA256 实现，避免引入额外的 crate
mod sha256 {
    pub fn digest(data: &[u8]) -> String {
        use std::ops::BitXor;

        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
            0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
        ];
        let k: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];

        let mut msg = data.to_vec();
        let bit_len = (msg.len() as u64) * 8;
        msg.push(0x80);
        while (msg.len() % 64) != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bit_len.to_be_bytes());

        for chunk in msg.chunks_exact(64) {
            let mut w = [0u32; 64];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7).bitxor(w[i - 15].rotate_right(18)).bitxor(w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17).bitxor(w[i - 2].rotate_right(19)).bitxor(w[i - 2] >> 10);
                w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
            }

            let mut a = h[0];
            let mut b = h[1];
            let mut c = h[2];
            let mut d = h[3];
            let mut e = h[4];
            let mut f = h[5];
            let mut g = h[6];
            let mut hh = h[7];

            for i in 0..64 {
                let s1 = e.rotate_right(6).bitxor(e.rotate_right(11)).bitxor(e.rotate_right(25));
                let ch = (e & f) ^ ((!e) & g);
                let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
                let s0 = a.rotate_right(2).bitxor(a.rotate_right(13)).bitxor(a.rotate_right(22));
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(maj);

                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }

            h[0] = h[0].wrapping_add(a);
            h[1] = h[1].wrapping_add(b);
            h[2] = h[2].wrapping_add(c);
            h[3] = h[3].wrapping_add(d);
            h[4] = h[4].wrapping_add(e);
            h[5] = h[5].wrapping_add(f);
            h[6] = h[6].wrapping_add(g);
            h[7] = h[7].wrapping_add(hh);
        }

        h.iter()
            .map(|v| format!("{:08x}", v))
            .collect::<String>()
    }
}
