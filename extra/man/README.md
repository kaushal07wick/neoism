# Neoism Man Pages

This directory contains manual pages for the Neoism terminal-first editor workspace in scdoc format.

## Files

- `neoism.1.scd` - Main Neoism manual page (section 1)
- `neoism.5.scd` - Neoism configuration file format manual page (section 5)  
- `neoism-bindings.5.scd` - Neoism key bindings manual page (section 5)

## Building

To build the man pages, you need `scdoc` installed:

### Install scdoc

**macOS (Homebrew):**
```bash
brew install scdoc
```

**Ubuntu/Debian:**
```bash
sudo apt install scdoc
```

**Arch Linux:**
```bash
sudo pacman -S scdoc
```

**From source:**
```bash
git clone https://git.sr.ht/~sircmpwn/scdoc
cd scdoc
make
sudo make install
```

### Build man pages

```bash
# Build all man pages
make -C extra/man

# Or build individually
scdoc < extra/man/neoism.1.scd > neoism.1
scdoc < extra/man/neoism.5.scd > neoism.5
scdoc < extra/man/neoism-bindings.5.scd > neoism-bindings.5
```

### Install man pages

```bash
# Install to system man directory (requires sudo)
sudo cp neoism.1 /usr/local/share/man/man1/
sudo cp neoism.5 /usr/local/share/man/man5/
sudo cp neoism-bindings.5 /usr/local/share/man/man5/

# Update man database
sudo mandb
```

### View man pages

```bash
man neoism
man 5 neoism
man 5 neoism-bindings
```

## Format

The man pages are written in scdoc format, which is a simple markup language for writing man pages. See the [scdoc documentation](https://git.sr.ht/~sircmpwn/scdoc) for syntax details.
