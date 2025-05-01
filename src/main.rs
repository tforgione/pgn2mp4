use std::collections::HashMap;
use std::env::args;
use std::fs::read_to_string;
use std::io::Write;
use std::path::Path;
use std::process::{exit, Child, ChildStdin, Command, Stdio};

use image::imageops::{resize, FilterType};
use image::{Rgba, RgbaImage};

use shakmaty::{Board, Chess, Color, Piece, Position, Role, Square};

use pgn_reader::{BufferedReader, SanPlus, Visitor};

fn draw_image(output: &mut RgbaImage, source: &RgbaImage, x: u32, y: u32, alpha: f32) {
    for i in 0..source.width() {
        for j in 0..source.height() {
            let new_pixel = source.get_pixel(i, j);
            if new_pixel[3] > 0 {
                let orig_pixel = output.get_pixel(x + i, y + j);

                let a = alpha * new_pixel[3] as f32 / 255.0;

                let r1 = orig_pixel[0] as f32 / 255.0;
                let r2 = new_pixel[0] as f32 / 255.0;
                let g1 = orig_pixel[1] as f32 / 255.0;
                let g2 = new_pixel[1] as f32 / 255.0;
                let b1 = orig_pixel[2] as f32 / 255.0;
                let b2 = new_pixel[2] as f32 / 255.0;

                let rf = (255.0 * (r1 * (1.0 - a) + a * r2)) as u8;
                let gf = (255.0 * (g1 * (1.0 - a) + a * g2)) as u8;
                let bf = (255.0 * (b1 * (1.0 - a) + a * b2)) as u8;

                let mix_pixel = Rgba([rf, gf, bf, 255]);
                *output.get_pixel_mut(x + i, y + j) = mix_pixel;
            }
        }
    }
}

pub fn get_name(role: Role, color: Color) -> String {
    let first = match color {
        Color::White => "w",
        Color::Black => "b",
    };

    let second = match role {
        Role::Pawn => "p",
        Role::Knight => "n",
        Role::Bishop => "b",
        Role::Rook => "r",
        Role::Queen => "q",
        Role::King => "k",
    };

    format!("{}{}.png", first, second)
}

pub struct PieceSet {
    pub pieces: HashMap<(Role, Color), RgbaImage>,
}

impl PieceSet {
    pub fn new<P: AsRef<Path>>(p: P, size: (u32, u32)) -> PieceSet {
        let mut pieces = HashMap::new();

        let p = p.as_ref();

        for role in Role::ALL {
            for color in Color::ALL {
                let image = image::open(p.join(get_name(role, color))).unwrap();
                let image = resize(&image, size.0, size.1, FilterType::Triangle);
                pieces.insert((role, color), image);
            }
        }

        PieceSet { pieces }
    }

    #[rustfmt::skip]
    pub fn default(width: u32, height: u32) -> PieceSet {
        use image::load_from_memory_with_format;
        use image::ImageFormat::Png;

        fn load(bytes: &[u8], width: u32, height: u32) -> RgbaImage {
            resize(&load_from_memory_with_format(bytes, Png).unwrap().to_rgba8(), width, height, FilterType::Triangle)
        }

        let mut pieces = HashMap::new();
        pieces.insert((Role::Pawn,   Color::White), load(include_bytes!("../assets/wp.png"), width, height));
        pieces.insert((Role::Knight, Color::White), load(include_bytes!("../assets/wn.png"), width, height));
        pieces.insert((Role::Bishop, Color::White), load(include_bytes!("../assets/wb.png"), width, height));
        pieces.insert((Role::Rook,   Color::White), load(include_bytes!("../assets/wr.png"), width, height));
        pieces.insert((Role::Queen,  Color::White), load(include_bytes!("../assets/wq.png"), width, height));
        pieces.insert((Role::King,   Color::White), load(include_bytes!("../assets/wk.png"), width, height));
        pieces.insert((Role::Pawn,   Color::Black), load(include_bytes!("../assets/bp.png"), width, height));
        pieces.insert((Role::Knight, Color::Black), load(include_bytes!("../assets/bn.png"), width, height));
        pieces.insert((Role::Bishop, Color::Black), load(include_bytes!("../assets/bb.png"), width, height));
        pieces.insert((Role::Rook,   Color::Black), load(include_bytes!("../assets/br.png"), width, height));
        pieces.insert((Role::Queen,  Color::Black), load(include_bytes!("../assets/bq.png"), width, height));
        pieces.insert((Role::King,   Color::Black), load(include_bytes!("../assets/bk.png"), width, height));

        PieceSet { pieces }
    }
}

pub struct FrameGenerator {
    pub background: RgbaImage,
    pub size: (u32, u32),
    pub frame: u32,
    pub board: Chess,
    pub side: Color,
    pub current_color: Color,
    pub transition: u32,
    pub pause: u32,
    pub pieces: PieceSet,
    pub child: Child,
    pub stdin: ChildStdin,
    pub current_move: u32,
    pub quiet: bool,
}

impl FrameGenerator {
    pub fn new(side: Color, size: (u32, u32), output: String, quiet: bool) -> FrameGenerator {
        let mut child = Command::new("ffmpeg")
            .arg("-loglevel")
            .arg("quiet")
            .arg("-f")
            .arg("rawvideo")
            .arg("-pix_fmt")
            .arg("rgba")
            .arg("-s")
            .arg(&format!("{}x{}", size.0, size.1))
            .arg("-r")
            .arg("60")
            .arg("-i")
            .arg("-")
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg(&output)
            .arg("-y")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();

        let image = image::load_from_memory_with_format(
            include_bytes!("../assets/chessboard.png"),
            image::ImageFormat::Png,
        )
        .unwrap()
        .to_rgba8();
        let image = resize(&image, size.0, size.1, FilterType::Triangle);

        FrameGenerator {
            frame: 0,
            size,
            background: image,
            board: Chess::new(),
            side,
            current_color: Color::White,
            transition: 20,
            pause: 50,
            pieces: PieceSet::default(size.0 / 8, size.1 / 8),
            child,
            stdin,
            current_move: 0,
            quiet,
        }
    }

    pub fn to_image(&self, board: &Board) -> RgbaImage {
        let mut image = self.background.clone();
        let width = image.width();
        let height = image.height();

        for rank in 0..8 {
            for line in 0..8 {
                let piece = board.piece_at(Square::new(rank * 8 + line));

                if let Some(Piece { role, color }) = piece {
                    let piece_image = self.pieces.pieces.get(&(role, color)).unwrap();

                    let (rank, line) = match self.side {
                        Color::Black => (rank, 7 - line),
                        Color::White => (7 - rank, line),
                    };

                    draw_image(
                        &mut image,
                        &piece_image,
                        (line as u32) * width / 8,
                        (rank as u32) * height / 8,
                        1.0,
                    );
                }
            }
        }

        image
    }

    pub fn save(&mut self, image: &RgbaImage, count: u32) {
        let bytes = image.as_raw();

        for _ in 0..count {
            self.stdin.write_all(bytes).unwrap();
            self.frame += 1;
        }
    }

    pub fn end(self) {}
}

impl Visitor for FrameGenerator {
    type Result = Result<(), ()>;

    fn begin_game(&mut self) {
        let image = self.to_image(self.board.board());
        self.save(&image, self.pause);
    }

    fn san(&mut self, san_plus: SanPlus) {
        let next_move = san_plus.san.to_move(&self.board).unwrap();

        if !self.quiet {
            if self.current_color == Color::White {
                self.current_move += 1;
                eprint!("{}.", self.current_move);
            }

            eprint!(" {}", san_plus);

            if self.current_color == Color::Black {
                eprintln!();
            }
        }

        let from = next_move.from().unwrap().coords();
        let to = next_move.to().coords();

        let from = match self.side {
            Color::White => (from.0 as u8, from.1 as u8),
            Color::Black => (7 - from.0 as u8, 7 - from.1 as u8),
        };

        let to = match self.side {
            Color::White => (to.0 as u8, to.1 as u8),
            Color::Black => (7 - to.0 as u8, 7 - to.1 as u8),
        };

        let mut clone = self.board.board().clone();
        clone.remove_piece_at(next_move.from().unwrap()).unwrap();

        // Remove captured piece
        let capture = self.board.board().piece_at(next_move.to());
        if capture.is_some() {
            clone.remove_piece_at(next_move.to()).unwrap();
        }

        let base_image = self.to_image(&clone);
        let width = base_image.width() as f32;
        let height = base_image.height() as f32;

        for i in 0..self.transition {
            let mut current_image = base_image.clone();
            let lambda = i as f32 / self.transition as f32;
            let x = (1.0 - lambda) * from.0 as f32 + lambda * to.0 as f32;
            let y = (1.0 - lambda) * from.1 as f32 + lambda * to.1 as f32;

            let piece_image = self
                .pieces
                .pieces
                .get(&(next_move.role(), self.current_color))
                .unwrap();

            draw_image(
                &mut current_image,
                &piece_image,
                (x * width / 8.0) as u32,
                ((7.0 - y) * height / 8.0) as u32,
                1.0,
            );

            if let Some(capture) = capture {
                let piece_image = self
                    .pieces
                    .pieces
                    .get(&(capture.role, capture.color))
                    .unwrap();

                draw_image(
                    &mut current_image,
                    &piece_image,
                    ((to.0 as f32) * width / 8.0) as u32,
                    ((7.0 - (to.1 as f32)) * height / 8.0) as u32,
                    1.0 - i as f32 / self.transition as f32,
                )
            }

            self.save(&current_image, 1);
        }

        let board = self.board.clone().play(&next_move).unwrap();
        self.board = board;

        let image = self.to_image(self.board.board());

        self.save(&image, self.pause);

        self.current_color = match self.current_color {
            Color::White => Color::Black,
            Color::Black => Color::White,
        };
    }

    fn end_game(&mut self) -> Self::Result {
        Ok(())
    }
}

fn main() {
    let mut args = args().skip(1);

    // Default value for args
    let mut display_as = Color::White;
    let mut size = (480, 480);
    let mut input_file = None;
    let mut output_file = None;
    let mut quiet = false;
    let mut pause = 50;
    let mut transition = 20;

    loop {
        match args.next().as_ref().map(String::as_str) {
            Some("-b") | Some("--black") => {
                display_as = Color::Black;
            }

            Some("-q") | Some("--quiet") => {
                quiet = true;
            }

            Some("-s") | Some("--size") => {
                let new_size = match args.next() {
                    None => {
                        eprintln!("error: expected video size");
                        exit(1);
                    }
                    Some(x) => x,
                };

                let split = new_size.split("x").collect::<Vec<_>>();
                if split.len() != 2 {
                    eprintln!("error: invalid video size");
                    exit(1);
                }

                let new_size = split
                    .into_iter()
                    .map(|x| x.parse::<u32>().ok())
                    .collect::<Option<Vec<_>>>();

                let new_size = match new_size {
                    None => {
                        eprintln!("error: invalid video size");
                        exit(1);
                    }
                    Some(x) => x,
                };

                size = (new_size[0], new_size[1]);
            }

            Some("-p") | Some("--pause") => {
                let new_pause = match args.next() {
                    None => {
                        eprintln!("error: expected pause duration");
                        exit(1);
                    }
                    Some(p) => p,
                };

                pause = match new_pause.parse::<u32>() {
                    Err(_) => {
                        eprintln!("error: invalid pause duration");
                        exit(1);
                    }
                    Ok(p) => p,
                };
            }

            Some("-t") | Some("--transition") => {
                let new_transition = match args.next() {
                    None => {
                        eprintln!("error: expected transition duration");
                        exit(1);
                    }
                    Some(p) => p,
                };

                transition = match new_transition.parse::<u32>() {
                    Err(_) => {
                        eprintln!("error: invalid transition duration");
                        exit(1);
                    }
                    Ok(p) => p,
                };
            }

            Some(filename) if input_file.is_none() => {
                input_file = Some(filename.to_string());
            }

            // input_file is necessarily some
            Some(filename) => {
                output_file = Some(filename.to_string());
            }

            None => break,
        }
    }

    let input_file = match input_file {
        None => {
            eprintln!("error: no input file specified");
            exit(1);
        }
        Some(f) => f,
    };

    let output_file = match output_file {
        None => {
            eprintln!("error: no output file specified");
            exit(1);
        }
        Some(f) => f,
    };

    let pgn = read_to_string(input_file).unwrap();
    let mut reader = BufferedReader::new_cursor(&pgn[..]);

    let mut frame_generator = FrameGenerator::new(display_as, size, output_file, quiet);
    frame_generator.pause = pause;
    frame_generator.transition = transition;

    reader.read_game(&mut frame_generator).unwrap();
    frame_generator.end();
}
