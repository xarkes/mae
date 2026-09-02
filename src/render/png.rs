pub fn write_png(path: &str, width: usize, height: usize, rgba: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, encode(width, height, rgba))
}

fn encode(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();

    buf.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&(width as u32).to_be_bytes());
    ihdr[4..8].copy_from_slice(&(height as u32).to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 6; // color type: RGBA
    write_chunk(&mut buf, b"IHDR", &ihdr);

    let mut raw = Vec::with_capacity(height * (1 + width * 4));
    for row in 0..height {
        raw.push(0); // filter: None
        raw.extend_from_slice(&rgba[row * width * 4..(row + 1) * width * 4]);
    }
    write_chunk(&mut buf, b"IDAT", &zlib_store(&raw));

    write_chunk(&mut buf, b"IEND", &[]);

    buf
}

fn write_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(chunk_type);
    buf.extend_from_slice(data);
    let mut crc: u32 = 0xFFFFFFFF;
    crc = crc32_update(crc, chunk_type);
    crc = crc32_update(crc, data);
    buf.extend_from_slice(&(crc ^ 0xFFFFFFFF).to_be_bytes());
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    // zlib header: 0x7801 is divisible by 31 (required by spec)
    buf.extend_from_slice(&[0x78, 0x01]);

    const MAX_BLOCK: usize = 65535;
    let mut offset = 0;
    loop {
        let end = (offset + MAX_BLOCK).min(data.len());
        let block = &data[offset..end];
        let is_last = end >= data.len();
        let len = block.len() as u16;
        buf.push(if is_last { 0x01 } else { 0x00 });
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(&(!len).to_le_bytes());
        buf.extend_from_slice(block);
        if is_last {
            break;
        }
        offset = end;
    }

    buf.extend_from_slice(&adler32(data).to_be_bytes());
    buf
}

fn adler32(data: &[u8]) -> u32 {
    const M: u32 = 65521;
    let (mut s1, mut s2) = (1u32, 0u32);
    for &b in data {
        s1 = (s1 + b as u32) % M;
        s2 = (s2 + s1) % M;
    }
    (s2 << 16) | s1
}

fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        let mut v = (crc ^ byte as u32) & 0xFF;
        for _ in 0..8 {
            v = if v & 1 != 0 {
                (v >> 1) ^ 0xEDB8_8320
            } else {
                v >> 1
            };
        }
        crc = (crc >> 8) ^ v;
    }
    crc
}
