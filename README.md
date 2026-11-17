# gob-cli

Given an image, find its address in the **Gallery of Babel**
([babelia.libraryofbabel.info](https://babelia.libraryofbabel.info)) — the gallery
that holds *every* 640×416 image drawn from a palette of 4096 colours. It also
works the other way round: from an address, it rebuilds the image.

```console
$ gob-cli encode photo.jpg > address.txt
$ head -c 60 address.txt; echo
1600016671984343619619267624976877615320...
```

That address is a number of about 961,758 digits. Anyone can paste it into the
gallery's search box and see exactly the same image, today and forever.

## Usage

```bash
# 1. an image's address (the number goes to stdout)
gob-cli encode photo.jpg

# with options
gob-cli encode photo.jpg --fill bars --border black --preview prepared.png

# 2. render an address back into an image
gob-cli decode --file address.txt output.png
cat address.txt | gob-cli decode --file - output.png

# 3. the 4096-colour palette, one hex colour per line
gob-cli palette
```

`decode` accepts the address as an argument, but a million-digit number is far
past the kernel's limit for command-line arguments, so use `--file` (`--file -`
reads from stdin).

### `encode` options

| Option | Effect |
| --- | --- |
| `--fill bars` (default) | keep proportions, pad with bars |
| `--fill stretch` | stretch the image to 640×416 |
| `--orient auto` (default) | rotate portrait photos the way the site does |
| `--orient landscape` / `portrait` | force the orientation |
| `--border <colour>` | bar colour: `#222222`, `#222`, or `grey`, `black`, `white`, `red`, `green`, `blue`, `yellow`, `orange`, `purple` |
| `--frame <n>` | pick a frame from a GIF or other animated image |
| `--preview <path>` | also save the quantized 640×416 image, which is what actually gets encoded |

## How it works

Every image in the gallery is a 640×416 grid, and every pixel is one of 4096
colours: 4 bits per channel (`#RGB`), i.e. `level * 17` in 8-bit terms.

The address is not a plain base-4096 conversion. The site runs a giant linear
congruential generator and then a Mersenne-Twister-like bit tempering:

```text
address --[ (a·x + c) mod m ]--> x --[ xor-shifts ]--> 266,240 colours
```

- `m` is 3,194,897 bits wide; `a`, `c` and `a⁻¹ mod m` are published constants.
- The tempering is four xor-shifts, clamped by `maskone` (2^2496009) and
  `masktwo` (2^1697289).
- Pixels are split into 8 quadrants of 160×208, each held in a 399,361-bit slot;
  pixel `i` occupies bits `12·i .. 12·i+11`, least significant first.

`gob-cli` implements both directions with `rug` (GMP), so **every image has an
address and every address has an image**. A full round trip takes a fraction of
a second.

This is a port of the algorithm published by the site's author, Jonathan
Basile: <https://github.com/librarianofbabel/libraryofbabel.info-algo>
(CC BY-SA-NC). The constants in `consts/` come from there; regenerate them with:

```bash
git clone https://github.com/librarianofbabel/libraryofbabel.info-algo /tmp/lobalgo
python3 tools/gen_consts.py /tmp/lobalgo
```

### Does it agree with the site's search?

Yes, whenever the image needs no resampling — and also for photos that only have
to be shrunk. Checked against the site's own CGI:

| Case | Result |
| --- | --- |
| 640×416 PNG using palette colours | identical address |
| 1200×900 photo (shrunk to 554×416) | identical address |
| 800×1400 portrait photo (rotated and shrunk) | 97.8 % of pixels identical; the rest are one palette step off |

The difference in the last case comes from ImageMagick's internal resampling,
which is not reproducible bit for bit: a few pixels land on either side of a
palette threshold. The address `gob-cli` prints is still valid and reproduces
exactly what `--preview` shows.

## Things worth knowing

- The gallery only holds 640×416 images from that palette, so larger photos are
  shrunk and quantized. `--preview` shows the exact result.
- The site stores each quadrant in a 399,361-bit slot but only fills 399,360 of
  those bits with pixels. So the permutation is not onto: an image has a single
  address, but that address may not be the one it originally came from. The site
  behaves the same way, and `gob-cli` reproduces that behaviour exactly.

## Building

```bash
cargo build --release      # binary at target/release/gob-cli
cargo test --release
```

Requires GMP and MPFR (`gmp` and `mpfr` on Arch, `libgmp-dev`/`libmpfr-dev` on
Debian).

## Licence

The algorithm and constants are the work of Jonathan Basile, published under
CC BY-SA-NC. This port keeps that licence.