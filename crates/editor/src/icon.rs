// 应用图标：纯代码绘制（不依赖图像文件）。
//
// - `app_icon_rgba`：任意尺寸的 RGBA 位图，供窗口/任务栏图标（main.rs）
// - `ico_bytes` / `write_ico`：多尺寸 ICO 文件（build.rs 生成 assets/app.ico）
//
// 画面（RPG Maker 存档修改器）：
// 蓝色渐变圆角方背景 + 白色软盘（存档）+ 金色魔法棒斜穿磁盘（修改器/金手指），
// 棒尖带四角星光，旁有两颗小星光。
// 本文件不依赖任何 crate，build.rs 通过 include! 复用绘制代码。

/// SDF 抗锯齿覆盖率：距离 0 处 50%，向内 1 像素平滑过渡到 100%
fn cov(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

/// 圆角矩形 SDF（坐标相对中心，y 向下为正）
fn sd_rrect(px: f32, py: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = px.abs() - hw + r;
    let qy = py.abs() - hh + r;
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// 直角矩形 SDF（圆角半径 0）
fn sd_rect(px: f32, py: f32, hw: f32, hh: f32) -> f32 {
    sd_rrect(px, py, hw, hh, 0.0)
}

/// 四角星光 SDF：竖直 + 水平两条细臂的并集（取 SDF 最小值）
fn sd_star(px: f32, py: f32, arm: f32, width: f32, r: f32) -> f32 {
    let v = sd_rrect(px, py, width, arm, r);
    let h = sd_rrect(px, py, arm, width, r);
    v.min(h)
}

/// 以直通 alpha 在 RGBA 上混合一层颜色（src-over，非预乘）
fn blend(dst: &mut [u8; 4], rgb: [u8; 3], a: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let ao = a + da * (1.0 - a);
    if ao <= 0.0 {
        return;
    }
    for c in 0..3 {
        let d = dst[c] as f32;
        dst[c] = ((rgb[c] as f32 * a + d * da * (1.0 - a)) / ao).round() as u8;
    }
    dst[3] = (ao * 255.0).round() as u8;
}

/// 绘制 size×size 图标位图（RGBA，圆角外透明）
pub fn app_icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let c45 = std::f32::consts::FRAC_1_SQRT_2; // cos45°
    let mut out = vec![0u8; (size * size * 4) as usize];
    for j in 0..size {
        for i in 0..size {
            let x = i as f32 + 0.5 - s * 0.5;
            let y = j as f32 + 0.5 - s * 0.5;
            let mut c = [0u8; 4];

            // 背景：圆角方 + 对角渐变（左上亮 → 右下深）+ 左上高光
            let t = ((i + j) as f32 / (2.0 * s)).clamp(0.0, 1.0);
            let bg = cov(sd_rrect(x, y, s * 0.5, s * 0.5, s * 0.22));
            let grad = [
                (116.0 + (34.0 - 116.0) * t) as u8,
                (184.0 + (110.0 - 184.0) * t) as u8,
                (240.0 + (190.0 - 240.0) * t) as u8,
            ];
            blend(&mut c, grad, bg);
            blend(&mut c, [255, 255, 255], bg * 0.16 * (1.0 - t));

            // 软盘阴影（右下方偏移）
            let shd = sd_rrect(x - s * 0.02, y - s * 0.047, s * 0.275, s * 0.24, s * 0.056);
            blend(&mut c, [10, 36, 80], cov(shd) * 0.3);

            // 磁盘本体：白色到浅灰蓝的纵向渐变 + 描边
            let dd = sd_rrect(x, y, s * 0.275, s * 0.24, s * 0.056);
            let dc = cov(dd);
            let ty = ((y / (s * 0.24)) * 0.5 + 0.5).clamp(0.0, 1.0);
            let body = [
                (255.0 + (214.0 - 255.0) * ty) as u8,
                (255.0 + (228.0 - 255.0) * ty) as u8,
                (255.0 + (241.0 - 255.0) * ty) as u8,
            ];
            blend(&mut c, body, dc);
            let ew = (s * 0.02).clamp(1.0, 2.0);
            blend(&mut c, [122, 148, 174], dc - cov(dd + ew));

            // 金属滑片（右上）
            let sh = sd_rrect(x - s * 0.101, y + s * 0.174, s * 0.168, s * 0.067, s * 0.031);
            let shc = cov(sh);
            blend(&mut c, [202, 215, 228], shc);
            // 滑片上的两条凹槽
            let lw = (s * 0.012).clamp(1.0, 2.0) * 0.5;
            let gr1 = sd_rect(x - s * 0.006, y + s * 0.174, lw, s * 0.056);
            let gr2 = sd_rect(x - s * 0.056, y + s * 0.174, lw, s * 0.056);
            blend(&mut c, [152, 170, 188], shc * cov(gr1) * 0.85);
            blend(&mut c, [152, 170, 188], shc * cov(gr2) * 0.85);

            // 标签（左下）
            let lb = sd_rrect(x + s * 0.112, y - s * 0.112, s * 0.162, s * 0.073, s * 0.036);
            let lbc = cov(lb);
            blend(&mut c, [241, 247, 252], lbc);
            // 标签上的三道白线
            let lw2 = (s * 0.02).clamp(1.0, 2.0) * 0.5;
            for k in 0..3 {
                let ly = s * (0.062 + 0.05 * k as f32);
                let line = sd_rect(x + s * 0.106, y - ly, s * 0.14, lw2);
                blend(&mut c, [255, 255, 255], lbc * cov(line) * 0.9);
            }

            // 魔法棒（修改器）：金色斜棒穿过磁盘，左下 → 右上
            // 旋转 45° 坐标系：u 垂直棒身（u<0 朝左上，受光面），v 沿棒身
            let u = (x + y) * c45;
            let v = (y - x) * c45;
            let rod_hw = (s * 0.038).max(1.8);
            let rod = sd_rrect(u, v, rod_hw, s * 0.38, rod_hw);
            let rc = cov(rod);
            // 金色渐变（棒尾深 → 棒尖亮）
            let tg = (0.5 - (v / (s * 0.38)) * 0.5).clamp(0.0, 1.0);
            let gold = [
                (232.0 + (248.0 - 232.0) * tg) as u8,
                (169.0 + (207.0 - 169.0) * tg) as u8,
                (58.0 + (94.0 - 58.0) * tg) as u8,
            ];
            blend(&mut c, gold, rc);
            // 描边
            let ew2 = (s * 0.015).clamp(1.0, 2.0);
            blend(&mut c, [163, 110, 26], rc - cov(rod + ew2));
            // 高光细条（左上侧）
            let hl = sd_rrect(
                u + s * 0.016,
                v,
                (s * 0.006).max(0.8),
                s * 0.36,
                (s * 0.006).max(0.8),
            );
            blend(&mut c, [255, 255, 255], rc * cov(hl) * 0.4);

            // 棒尖四角星光（右上）
            let sx = s * 0.27;
            let sy = -s * 0.27;
            // 光晕
            let gd = sd_star(x - sx, y - sy, s * 0.13, s * 0.05, s * 0.022);
            blend(&mut c, [255, 233, 176], cov(gd) * 0.25);
            // 星体
            let sd = sd_star(x - sx, y - sy, s * 0.1, s * 0.024, s * 0.012);
            blend(&mut c, [255, 214, 105], cov(sd));
            // 星心
            let sc = sd_star(x - sx, y - sy, s * 0.06, s * 0.013, s * 0.007);
            blend(&mut c, [255, 255, 255], cov(sc) * 0.9);

            // 两颗小星光：沿魔法棒对角线对称分布（左上/右下），与棒尖星光同风格（光晕 + 星体）
            for (mx, my) in [(-0.30f32, -0.42f32), (0.42, 0.30)] {
                // 光晕
                let mg = sd_star(x - mx * s, y - my * s, s * 0.055, s * 0.02, s * 0.01);
                blend(&mut c, [255, 233, 176], cov(mg) * 0.2);
                // 星体
                let ms = sd_star(
                    x - mx * s,
                    y - my * s,
                    (s * 0.04).max(2.4),
                    (s * 0.012).max(1.0),
                    (s * 0.008).max(0.5),
                );
                blend(&mut c, [255, 226, 140], cov(ms) * 0.95);
            }

            let idx = ((j * size + i) * 4) as usize;
            out[idx..idx + 4].copy_from_slice(&c);
        }
    }
    out
}

/// 生成多尺寸 ICO 文件字节（16/24/32/48/64/128/256，32 位 BGRA）
pub fn ico_bytes() -> Vec<u8> {
    let sizes: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];
    let mut out = Vec::new();
    // ICONDIR
    out.extend(0u16.to_le_bytes()); // 保留
    out.extend(1u16.to_le_bytes()); // 类型：图标
    out.extend((sizes.len() as u16).to_le_bytes()); // 条目数
    let mut offset = 6 + 16 * sizes.len() as u32;
    // 先写全部 ICONDIRENTRY，再按偏移追加图像数据
    for &s in &sizes {
        let (w, h) = if s >= 256 { (0u8, 0u8) } else { (s as u8, s as u8) };
        out.push(w);
        out.push(h);
        out.push(0); // 调色板颜色数
        out.push(0); // 保留
        out.extend(1u16.to_le_bytes()); // 颜色平面
        out.extend(32u16.to_le_bytes()); // 位深
        out.extend((blob_len(s)).to_le_bytes()); // 图像字节数
        out.extend(offset.to_le_bytes()); // 图像偏移
        offset += blob_len(s);
    }
    for &s in &sizes {
        out.extend(blob(s));
    }
    out
}

/// 单个尺寸的 DIB 数据长度：BITMAPINFOHEADER(40) + 像素 + AND 掩码
fn blob_len(s: u32) -> u32 {
    40 + s * s * 4 + ((s + 31) / 32) * 4 * s
}

/// 单个尺寸的 DIB 图像数据
fn blob(s: u32) -> Vec<u8> {
    let rgba = app_icon_rgba(s);
    let mut img = Vec::with_capacity(blob_len(s) as usize);
    // BITMAPINFOHEADER（40 字节）
    img.extend(40u32.to_le_bytes()); // biSize
    img.extend(s.to_le_bytes()); // biWidth
    img.extend((s * 2).to_le_bytes()); // biHeight（XOR 位图 + AND 掩码）
    img.extend(1u16.to_le_bytes()); // biPlanes
    img.extend(32u16.to_le_bytes()); // biBitCount
    img.extend([0u8; 24]); // 压缩/尺寸/分辨率/调色板
    // 像素：RGBA → BGRA，自底向上
    for row in (0..s).rev() {
        for col in 0..s {
            let i = ((row * s + col) * 4) as usize;
            img.push(rgba[i + 2]);
            img.push(rgba[i + 1]);
            img.push(rgba[i]);
            img.push(rgba[i + 3]);
        }
    }
    // AND 掩码（全 0，逐行按 32 位对齐）
    let mask_row = ((s + 31) / 32) * 4;
    img.extend(std::iter::repeat(0u8).take((mask_row * s) as usize));
    img
}

/// 把多尺寸图标写入 .ico 文件
#[allow(dead_code)]
pub fn write_ico(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::write(path, ico_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_disk_wand_and_transparent_corners() {
        let rgba = app_icon_rgba(64);
        let px = |i: u32, j: u32| -> [u8; 4] {
            let o = ((j * 64 + i) * 4) as usize;
            [rgba[o], rgba[o + 1], rgba[o + 2], rgba[o + 3]]
        };
        // 四角透明（圆角背景之外）
        for (i, j) in [(0, 0), (63, 0), (0, 63), (63, 63)] {
            assert_eq!(px(i, j)[3], 0, "({i},{j}) 角应透明");
        }
        // 磁盘白色区域（中心右侧，魔法棒之外）
        let disk = px(44, 32);
        assert_eq!(disk[3], 255);
        assert!(
            disk[0] > 200 && disk[1] > 200 && disk[2] > 200,
            "磁盘应为白色: {disk:?}"
        );
        // 中心被魔法棒斜穿 → 金色
        let wand = px(32, 32);
        assert!(
            wand[0] > 200 && wand[1] > 140 && wand[2] < 180 && wand[0] > wand[2],
            "中心应是金色魔法棒: {wand:?}"
        );
        // 棒尖四角星光（右上，竖直臂外侧远离星心）
        let star = px(49, 9);
        assert!(
            star[0] > 240 && star[1] > 190 && star[2] < 160,
            "棒尖应是金色星光: {star:?}"
        );
        // 右上滑片区是灰蓝色（避开斜穿的魔法棒）
        let shutter = px(34, 22);
        assert!(
            shutter[0] < 240 && shutter[1] < 240 && shutter[2] >= shutter[0],
            "滑片区应是灰蓝色: {shutter:?}"
        );
        // 背景是蓝色渐变
        let bg = px(32, 10);
        assert!(
            bg[2] > bg[0] && bg[0] > 30,
            "背景应为蓝色渐变: {bg:?}"
        );
    }

    #[test]
    fn ico_header_is_valid() {
        let b = ico_bytes();
        // ICONDIR：保留 0、类型 1、7 个条目
        assert_eq!(&b[0..2], &[0, 0]);
        assert_eq!(&b[2..4], &[1, 0]);
        assert_eq!(u16::from_le_bytes([b[4], b[5]]), 7);
        // 条目 0（16×16）：图像偏移紧跟条目表
        let size = u32::from_le_bytes(b[14..18].try_into().unwrap());
        let off = u32::from_le_bytes(b[18..22].try_into().unwrap());
        assert!(size > 16 * 16 * 4 && off == 6 + 16 * 7);
        // 256 尺寸条目：宽高字节为 0
        let e6 = 6 + 16 * 6;
        assert_eq!(b[e6], 0);
        assert_eq!(b[e6 + 1], 0);
        // 条目间偏移连续（条目表之后直接是图像），最后一块图像结束于文件末尾
        let last_size = u32::from_le_bytes(b[e6 + 8..e6 + 12].try_into().unwrap());
        let last_off = u32::from_le_bytes(b[e6 + 12..e6 + 16].try_into().unwrap());
        let expected_last_off = 6 + 16 * 7
            + blob_len(16) + blob_len(24) + blob_len(32) + blob_len(48) + blob_len(64) + blob_len(128);
        assert_eq!(last_off, expected_last_off, "最后一条目的偏移应紧跟前面所有图像");
        assert_eq!(b.len() as u32, last_off + last_size, "图像应到文件末尾");
    }
}
