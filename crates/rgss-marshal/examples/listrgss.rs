fn read_u32(b: &[u8], pos: &mut usize) -> u32 {
    let v = u32::from_le_bytes([b[*pos], b[*pos + 1], b[*pos + 2], b[*pos + 3]]);
    *pos += 4;
    v
}
fn main() {
    let archive = std::fs::read("RMVXA_test/Game.rgss3a").unwrap();
    let mut pos = 8usize;
    let raw = read_u32(&archive, &mut pos);
    let magic = raw.wrapping_mul(9).wrapping_add(3);
    loop {
        let body_offset = read_u32(&archive, &mut pos) ^ magic;
        if body_offset == 0 { break; }
        let entry_len = read_u32(&archive, &mut pos) ^ magic;
        let _fm = read_u32(&archive, &mut pos) ^ magic;
        let path_len = read_u32(&archive, &mut pos) ^ magic;
        let path: Vec<u8> = archive[pos..pos + path_len as usize]
            .iter().enumerate()
            .map(|(i, b)| b ^ (magic >> (8 * (i % 4))) as u8)
            .collect();
        pos += path_len as usize;
        println!("{} ({} 字节)", String::from_utf8_lossy(&path), entry_len);
    }
}