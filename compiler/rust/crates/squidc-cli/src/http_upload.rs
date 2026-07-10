use std::{
    fs::File,
    io::{Seek, SeekFrom},
    path::Path,
};

const OFFSET_HEADER: &str = "X-Squid-Upload-Offset";
const TOTAL_HEADER: &str = "X-Squid-Upload-Total";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HttpUploadResult {
    pub bytes_sent: usize,
    pub resumed_bytes: usize,
}

pub fn upload(
    host: &str,
    port: u16,
    source: &Path,
    name: &str,
) -> Result<HttpUploadResult, String> {
    let mut file = File::open(source)
        .map_err(|error| format!("failed to open {}: {error}", source.display()))?;
    let total = file
        .metadata()
        .map_err(|error| format!("failed to stat {}: {error}", source.display()))?
        .len();
    let url = format!("http://{host}:{port}/upload/{name}");

    let head = ureq::head(&url)
        .call()
        .map_err(|error| format!("upload HEAD {url} failed: {error}"))?;
    let offset = response_u64_header(&head, OFFSET_HEADER)?.unwrap_or(0);
    let reported_total = response_u64_header(&head, TOTAL_HEADER)?;
    if offset > total {
        return Err(format!(
            "device upload offset {offset} exceeds local file size {total}"
        ));
    }
    if let Some(reported_total) = reported_total {
        if offset > 0 && reported_total != total {
            return Err(format!(
                "device upload total {reported_total} does not match local file size {total}"
            ));
        }
    }
    if offset == total {
        return Ok(HttpUploadResult {
            bytes_sent: 0,
            resumed_bytes: usize::try_from(offset)
                .map_err(|_| "upload offset does not fit host usize".to_string())?,
        });
    }

    file.seek(SeekFrom::Start(offset))
        .map_err(|error| format!("failed to seek {}: {error}", source.display()))?;
    let remaining = total - offset;
    let mut request = ureq::put(&url).header("Content-Length", remaining.to_string());
    if offset > 0 {
        request = request.header(
            "Content-Range",
            format!("bytes {offset}-{}/{total}", total - 1),
        );
    }
    let mut response = request
        .send(file)
        .map_err(|error| format!("upload PUT {url} failed: {error}"))?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("failed to read upload response: {error}"))?;
    if body != "ok\n" {
        return Err(format!("upload PUT returned unexpected body: {body:?}"));
    }
    Ok(HttpUploadResult {
        bytes_sent: usize::try_from(remaining)
            .map_err(|_| "upload size does not fit host usize".to_string())?,
        resumed_bytes: usize::try_from(offset)
            .map_err(|_| "upload offset does not fit host usize".to_string())?,
    })
}

fn response_u64_header(
    response: &ureq::http::Response<ureq::Body>,
    name: &str,
) -> Result<Option<u64>, String> {
    let Some(value) = response.headers().get(name) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|error| format!("invalid {name} header: {error}"))?;
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|error| format!("invalid {name} header {value:?}: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn resumes_http_upload_from_head_offset() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut head, _) = listener.accept().unwrap();
            let head_request = read_request(&mut head);
            assert!(head_request.starts_with("HEAD /upload/book.binbook HTTP/1.1\r\n"));
            head.write_all(
                b"HTTP/1.1 200 OK\r\nX-Squid-Upload-Offset: 3\r\nX-Squid-Upload-Total: 6\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();

            let (mut put, _) = listener.accept().unwrap();
            let put_request = read_request(&mut put);
            assert!(put_request.starts_with("PUT /upload/book.binbook HTTP/1.1\r\n"));
            assert!(put_request.contains("content-range: bytes 3-5/6\r\n"));
            assert!(put_request.ends_with("def"));
            put.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nok\n")
                .unwrap();
        });

        let mut source = std::env::temp_dir();
        source.push(format!(
            "squidc-http-upload-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&source, b"abcdef").unwrap();
        let result = upload("127.0.0.1", port, &source, "book.binbook").unwrap();
        fs::remove_file(source).unwrap();
        server.join().unwrap();
        assert_eq!(
            result,
            HttpUploadResult {
                bytes_sent: 3,
                resumed_bytes: 3,
            }
        );
    }

    fn read_request(stream: &mut impl Read) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 1024];
        let mut body_len = None;
        loop {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0, "connection closed before request completed");
            bytes.extend_from_slice(&buffer[..count]);
            if body_len.is_none() {
                if let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..header_end + 4]);
                    let content_len = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    body_len = Some((header_end + 4, content_len));
                }
            }
            if let Some((header_end, content_len)) = body_len {
                if bytes.len() >= header_end + content_len {
                    return String::from_utf8(bytes).unwrap();
                }
            }
        }
    }
}
