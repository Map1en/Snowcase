use std::io::{self, Read, Seek, SeekFrom};

pub const QUICK_SPAN: u64 = 1 << 20;

pub fn quick_fingerprint(mut reader: impl Read + Seek, size: u64) -> io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size.to_le_bytes());
    let mut head = Vec::new();
    (&mut reader).take(QUICK_SPAN.min(size)).read_to_end(&mut head)?;
    hasher.update(&head);
    if size > QUICK_SPAN {
        let tail = QUICK_SPAN.min(size - QUICK_SPAN);
        reader.seek(SeekFrom::Start(size - tail))?;
        let mut rest = Vec::new();
        reader.take(tail).read_to_end(&mut rest)?;
        hasher.update(&rest);
    }
    Ok(format!("b3:{}", hasher.finalize().to_hex()))
}

pub fn crc32(mut reader: impl Read) -> io::Result<u32> {
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(hasher.finalize());
        }
        hasher.update(&buffer[..read]);
    }
}

#[cfg(test)]
mod tests;
