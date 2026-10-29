//! AggroChess NNUE trainer.
//!
//! Network: (768 -> H) x 2 perspectives -> SCReLU -> 1, with 8 output buckets chosen by the
//! number of pieces on the board. Trained on AggroChess's own self-play data (see
//! `src/datagen.rs` in the engine for the record format).
//!
//! train <out.nnue> <epochs> <lr> <wdl> <file>...   train and write a quantised net
//! eval  <net.nnue> <fen>                            quantised evaluation of a position

use std::io::{Read, Write};

const INPUTS: usize = 768;
const H: usize = 256;
const BUCKETS: usize = 8;
const SCALE: f32 = 400.0;
const QA: i32 = 255;
const QB: i32 = 64;
const CLIP: f32 = 1.98;
const RECORD: usize = 32;

#[derive(Clone)]
struct Net {
    ft_w: Vec<f32>,
    ft_b: Vec<f32>,
    out_w: Vec<f32>,
    out_b: Vec<f32>,
}

impl Net {
    fn zeros() -> Self {
        Net { ft_w: vec![0.0; INPUTS * H], ft_b: vec![0.0; H], out_w: vec![0.0; BUCKETS * 2 * H], out_b: vec![0.0; BUCKETS] }
    }
    fn random(seed: u64) -> Self {
        let mut r = Rng(seed | 1);
        let mut n = Net::zeros();
        let s1 = (1.0 / 32.0f32).sqrt();
        for w in n.ft_w.iter_mut() {
            *w = r.gauss() * s1;
        }
        let s2 = (1.0 / (2 * H) as f32).sqrt();
        for w in n.out_w.iter_mut() {
            *w = r.gauss() * s2;
        }
        n
    }
    fn params_mut(&mut self) -> [&mut Vec<f32>; 4] {
        [&mut self.ft_w, &mut self.ft_b, &mut self.out_w, &mut self.out_b]
    }
    fn zero(&mut self) {
        for p in self.params_mut() {
            p.iter_mut().for_each(|x| *x = 0.0);
        }
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    fn gauss(&mut self) -> f32 {
        let u1 = self.unit().max(1e-7);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
    }
}

/// One decoded training position.
struct Sample {
    stm: [u16; 32],
    nstm: [u16; 32],
    n: usize,
    bucket: usize,
    score: f32, // side to move's view, cp
    wdl: f32,   // side to move's view, 0..1
}

#[inline]
fn feature(persp: usize, color: usize, piece: usize, sq: usize) -> u16 {
    let s = if persp == 0 { sq } else { sq ^ 56 };
    ((if color == persp { 0 } else { 384 }) + piece * 64 + s) as u16
}

fn bucket_for(count: usize) -> usize {
    ((count.max(2) - 2) / 4).min(BUCKETS - 1)
}

fn decode(rec: &[u8]) -> Sample {
    let occ = u64::from_le_bytes(rec[0..8].try_into().unwrap());
    let score = i16::from_le_bytes([rec[24], rec[25]]) as f32;
    let result = rec[26] as f32 / 2.0;
    let stm = rec[27] as usize;
    let mut s = Sample { stm: [0; 32], nstm: [0; 32], n: 0, bucket: 0, score: 0.0, wdl: 0.0 };
    let mut it = occ;
    let mut i = 0;
    while it != 0 && i < 32 {
        let sq = it.trailing_zeros() as usize;
        it &= it - 1;
        let nib = (rec[8 + i / 2] >> (4 * (i % 2))) & 15;
        let color = (nib >> 3) as usize;
        let piece = (nib & 7) as usize;
        s.stm[i] = feature(stm, color, piece, sq);
        s.nstm[i] = feature(1 - stm, color, piece, sq);
        i += 1;
    }
    s.n = i;
    s.bucket = bucket_for(i);
    if stm == 0 {
        s.score = score;
        s.wdl = result;
    } else {
        s.score = -score;
        s.wdl = 1.0 - result;
    }
    s
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Forward + backward for one sample; accumulates gradients into `g`. Returns the loss.
fn train_one(net: &Net, g: &mut Net, s: &Sample, wdl_w: f32, acc: &mut [f32; 2 * H]) -> f32 {
    let (a, b) = acc.split_at_mut(H);
    a.copy_from_slice(&net.ft_b);
    b.copy_from_slice(&net.ft_b);
    for k in 0..s.n {
        let fa = s.stm[k] as usize * H;
        let fb = s.nstm[k] as usize * H;
        for (x, w) in a.iter_mut().zip(&net.ft_w[fa..fa + H]) {
            *x += *w;
        }
        for (x, w) in b.iter_mut().zip(&net.ft_w[fb..fb + H]) {
            *x += *w;
        }
    }
    let ow = &net.out_w[s.bucket * 2 * H..(s.bucket + 1) * 2 * H];
    let mut y = net.out_b[s.bucket];
    for i in 0..2 * H {
        let v = acc[i].clamp(0.0, 1.0);
        y += v * v * ow[i];
    }
    let p = sigmoid(y);
    let t = (1.0 - wdl_w) * sigmoid(s.score / SCALE) + wdl_w * s.wdl;
    let loss = (p - t) * (p - t);
    let d = 2.0 * (p - t) * p * (1.0 - p);

    g.out_b[s.bucket] += d;
    let gow = &mut g.out_w[s.bucket * 2 * H..(s.bucket + 1) * 2 * H];
    let mut dacc = [0f32; 2 * H];
    for i in 0..2 * H {
        let x = acc[i];
        let v = x.clamp(0.0, 1.0);
        gow[i] += d * v * v;
        if x > 0.0 && x < 1.0 {
            dacc[i] = d * ow[i] * 2.0 * x;
        }
    }
    let (da, db) = dacc.split_at(H);
    for (gb, (x, y)) in g.ft_b.iter_mut().zip(da.iter().zip(db)) {
        *gb += *x + *y;
    }
    for k in 0..s.n {
        let fa = s.stm[k] as usize * H;
        for (gw, x) in g.ft_w[fa..fa + H].iter_mut().zip(da) {
            *gw += *x;
        }
        let fb = s.nstm[k] as usize * H;
        for (gw, x) in g.ft_w[fb..fb + H].iter_mut().zip(db) {
            *gw += *x;
        }
    }
    loss
}

fn eval_float(net: &Net, s: &Sample) -> f32 {
    let mut acc = [0f32; 2 * H];
    let mut g = Net::zeros(); // unused
    let _ = &mut g;
    let (a, b) = acc.split_at_mut(H);
    a.copy_from_slice(&net.ft_b);
    b.copy_from_slice(&net.ft_b);
    for k in 0..s.n {
        let fa = s.stm[k] as usize * H;
        let fb = s.nstm[k] as usize * H;
        for i in 0..H {
            a[i] += net.ft_w[fa + i];
            b[i] += net.ft_w[fb + i];
        }
    }
    let ow = &net.out_w[s.bucket * 2 * H..(s.bucket + 1) * 2 * H];
    let mut y = net.out_b[s.bucket];
    for i in 0..2 * H {
        let v = acc[i].clamp(0.0, 1.0);
        y += v * v * ow[i];
    }
    y
}

struct Adam {
    m: Net,
    v: Net,
    t: i32,
}

impl Adam {
    fn step(&mut self, net: &mut Net, g: &Net, lr: f32, scale: f32) {
        self.t += 1;
        let (b1, b2, eps) = (0.9f32, 0.999f32, 1e-8f32);
        let c1 = 1.0 - b1.powi(self.t);
        let c2 = 1.0 - b2.powi(self.t);
        let gs = [&g.ft_w, &g.ft_b, &g.out_w, &g.out_b];
        let ms = self.m.params_mut();
        let vs = self.v.params_mut();
        let ps = net.params_mut();
        for (((p, gg), m), v) in ps.into_iter().zip(gs).zip(ms).zip(vs) {
            for i in 0..p.len() {
                let gr = gg[i] * scale;
                m[i] = b1 * m[i] + (1.0 - b1) * gr;
                v[i] = b2 * v[i] + (1.0 - b2) * gr * gr;
                let upd = (m[i] / c1) / ((v[i] / c2).sqrt() + eps);
                p[i] = (p[i] - lr * upd).clamp(-CLIP, CLIP);
            }
        }
    }
}

fn load(files: &[String]) -> Vec<u8> {
    let mut all = Vec::new();
    for f in files {
        let mut v = Vec::new();
        std::fs::File::open(f).unwrap().read_to_end(&mut v).unwrap();
        let n = v.len() / RECORD * RECORD;
        all.extend_from_slice(&v[..n]);
    }
    all
}

fn write_quantised(net: &Net, path: &str) {
    let mut out = Vec::new();
    out.extend_from_slice(b"AGNN");
    out.extend_from_slice(&(H as u32).to_le_bytes());
    let q = |x: f32, s: i32| -> i16 { (x * s as f32).round().clamp(-32767.0, 32767.0) as i16 };
    for &w in &net.ft_w {
        out.extend_from_slice(&q(w, QA).to_le_bytes());
    }
    for &w in &net.ft_b {
        out.extend_from_slice(&q(w, QA).to_le_bytes());
    }
    for &w in &net.out_w {
        out.extend_from_slice(&q(w, QB).to_le_bytes());
    }
    for &w in &net.out_b {
        out.extend_from_slice(&((w * (QA * QB) as f32).round() as i32).to_le_bytes());
    }
    std::fs::File::create(path).unwrap().write_all(&out).unwrap();
}

fn save_float(net: &Net, path: &str) {
    let mut out = Vec::new();
    for p in [&net.ft_w, &net.ft_b, &net.out_w, &net.out_b] {
        for &x in p.iter() {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    std::fs::File::create(path).unwrap().write_all(&out).unwrap();
}

fn load_float(path: &str) -> Net {
    let mut v = Vec::new();
    std::fs::File::open(path).unwrap().read_to_end(&mut v).unwrap();
    let mut n = Net::zeros();
    let mut off = 0;
    for p in n.params_mut() {
        for x in p.iter_mut() {
            *x = f32::from_le_bytes(v[off..off + 4].try_into().unwrap());
            off += 4;
        }
    }
    n
}

fn train(args: &[String]) {
    let out = &args[0];
    let epochs: usize = args[1].parse().unwrap();
    let lr0: f32 = args[2].parse().unwrap();
    let wdl_w: f32 = args[3].parse().unwrap();
    let mut files: Vec<String> = args[4..].to_vec();
    let resume = files.iter().position(|f| f.ends_with(".f32")).map(|i| files.remove(i));
    let threads = std::env::var("THREADS").ok().and_then(|s| s.parse().ok()).unwrap_or(2usize);
    let batch = 16384usize;

    let data = load(&files);
    let n = data.len() / RECORD;
    eprintln!("{} positions", n);
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let mut order: Vec<u32> = (0..n as u32).collect();
    for i in (1..n).rev() {
        let j = (rng.next() % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    let nval = (n / 100).min(50_000);
    let (val, train_idx) = order.split_at(nval);
    let val: Vec<u32> = val.to_vec();
    let mut train_idx: Vec<u32> = train_idx.to_vec();

    let mut net = match &resume {
        Some(p) => load_float(p),
        None => Net::random(42),
    };
    let mut adam = Adam { m: Net::zeros(), v: Net::zeros(), t: 0 };
    let mut grads: Vec<Net> = (0..threads).map(|_| Net::zeros()).collect();
    let steps_per_epoch = train_idx.len() / batch;
    let total_steps = steps_per_epoch * epochs;
    let mut step = 0usize;
    let start = std::time::Instant::now();

    for epoch in 0..epochs {
        for i in (1..train_idx.len()).rev() {
            let j = (rng.next() % (i as u64 + 1)) as usize;
            train_idx.swap(i, j);
        }
        let mut epoch_loss = 0f64;
        for b in 0..steps_per_epoch {
            // cosine decay down to 1% of the start rate
            let progress = step as f32 / total_steps as f32;
            let lr = lr0 * (0.01 + 0.99 * 0.5 * (1.0 + (std::f32::consts::PI * progress).cos()));
            let idx = &train_idx[b * batch..(b + 1) * batch];
            let chunk = batch / threads;
            let losses: Vec<f32> = std::thread::scope(|sc| {
                let mut hs = Vec::new();
                for (t, g) in grads.iter_mut().enumerate() {
                    let part = &idx[t * chunk..(t + 1) * chunk];
                    let net = &net;
                    let data = &data;
                    hs.push(sc.spawn(move || {
                        g.zero();
                        let mut acc = [0f32; 2 * H];
                        let mut l = 0f32;
                        for &k in part {
                            let k = k as usize;
                            let s = decode(&data[k * RECORD..(k + 1) * RECORD]);
                            l += train_one(net, g, &s, wdl_w, &mut acc);
                        }
                        l
                    }));
                }
                hs.into_iter().map(|h| h.join().unwrap()).collect()
            });
            epoch_loss += losses.iter().map(|&x| x as f64).sum::<f64>();
            // sum thread gradients into grads[0]
            let (first, rest) = grads.split_at_mut(1);
            for g in rest.iter() {
                for (a, b) in first[0].params_mut().into_iter().zip([&g.ft_w, &g.ft_b, &g.out_w, &g.out_b]) {
                    for (x, y) in a.iter_mut().zip(b.iter()) {
                        *x += *y;
                    }
                }
            }
            adam.step(&mut net, &first[0], lr, 1.0 / batch as f32);
            step += 1;
        }
        // validation
        let mut vl = 0f64;
        for &k in &val {
            let k = k as usize;
            let s = decode(&data[k * RECORD..(k + 1) * RECORD]);
            let p = sigmoid(eval_float(&net, &s));
            let t = (1.0 - wdl_w) * sigmoid(s.score / SCALE) + wdl_w * s.wdl;
            vl += ((p - t) * (p - t)) as f64;
        }
        eprintln!(
            "epoch {:>3}  train {:.6}  val {:.6}  {:.0}s",
            epoch + 1,
            epoch_loss / (steps_per_epoch * batch) as f64,
            vl / val.len() as f64,
            start.elapsed().as_secs_f64()
        );
        write_quantised(&net, out);
        save_float(&net, &format!("{}.f32", out));
    }
}

// ---- quantised inference (must match the engine) ----

fn parse_fen(fen: &str) -> Option<(Vec<(usize, usize, usize)>, usize)> {
    let mut parts = fen.split_whitespace();
    let placement = parts.next()?;
    let stm = if parts.next()? == "w" { 0 } else { 1 };
    let mut pcs = Vec::new();
    let mut rank = 7i32;
    let mut file = 0i32;
    for ch in placement.chars() {
        match ch {
            '/' => {
                rank -= 1;
                file = 0;
            }
            '1'..='8' => file += ch as i32 - '0' as i32,
            _ => {
                let color = if ch.is_ascii_uppercase() { 0 } else { 1 };
                let piece = "pnbrqk".find(ch.to_ascii_lowercase())?;
                pcs.push((color, piece, (rank * 8 + file) as usize));
                file += 1;
            }
        }
    }
    Some((pcs, stm))
}

fn eval_quantised(path: &str, fen: &str) -> i32 {
    let mut v = Vec::new();
    std::fs::File::open(path).unwrap().read_to_end(&mut v).unwrap();
    let h = u32::from_le_bytes(v[4..8].try_into().unwrap()) as usize;
    let mut off = 8;
    let mut rd = |n: usize| -> Vec<i16> {
        let r = (0..n).map(|i| i16::from_le_bytes([v[off + 2 * i], v[off + 2 * i + 1]])).collect();
        off += 2 * n;
        r
    };
    let ft_w = rd(INPUTS * h);
    let ft_b = rd(h);
    let out_w = rd(BUCKETS * 2 * h);
    let out_b: Vec<i32> = (0..BUCKETS).map(|i| i32::from_le_bytes(v[off + 4 * i..off + 4 * i + 4].try_into().unwrap())).collect();
    let (pcs, stm) = parse_fen(fen).unwrap();
    let mut acc = [ft_b.clone(), ft_b.clone()];
    for &(c, p, s) in &pcs {
        for persp in 0..2 {
            let f = feature(persp, c, p, s) as usize;
            for i in 0..h {
                acc[persp][i] += ft_w[f * h + i];
            }
        }
    }
    let bucket = bucket_for(pcs.len());
    let ow = &out_w[bucket * 2 * h..(bucket + 1) * 2 * h];
    let mut sum: i64 = 0;
    for (k, persp) in [stm, 1 - stm].into_iter().enumerate() {
        for i in 0..h {
            let x = (acc[persp][i] as i32).clamp(0, QA) as i64;
            sum += x * x * ow[k * h + i] as i64;
        }
    }
    let y = sum / QA as i64 + out_b[bucket] as i64;
    (y * SCALE as i64 / (QA * QB) as i64) as i32
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("train") => train(&args[2..]),
        Some("eval") => println!("{}", eval_quantised(&args[2], &args[3])),
        _ => eprintln!("usage: train <out> <epochs> <lr> <wdl> <files...> | eval <net> <fen>"),
    }
}
