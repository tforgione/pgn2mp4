# pgn2mp4

*Easily convert chess games into videos.*

https://github.com/user-attachments/assets/59129378-c0b3-4284-92ae-30bffa887322

### Installation

First install [Rust](https://www.rust-lang.org/tools/install) and `ffmpeg`.

Then, you can install `pgn2mp4` by running

```sh
cargo install pgn2mp4
```

### Usage

Simply run

```
pgn2mp4 input.pgn ouptut.mp4
```

### Options

- `-b`, `--black`: renders the video with the black's perspective
- `-s`, `--size`: size of the final video
- `-p`, `--pause`: number of frames between each move
- `-t`, `--transition`: number of frames for each piece motion
- `-q`, `--quiet`: disable printing moves on stderr

