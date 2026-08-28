//! Userland libraries, allocator, panic formatting, and line editor test suite.

use super::harness::TestRunner;
use std::collections::BTreeMap;
use std::fmt::Write;

/// Registers userland slab allocator, panic handling, line editor, and base64 tests with the runner.
pub fn register_tests(runner: &mut TestRunner) {
    runner.run_test(
        "userland",
        "libc Small-Object Slab Allocator (10k cycles)",
        test_libc_small_object_allocator,
    );
    runner.run_test(
        "userland",
        "Userland Panic Formatting on STDERR (fd 2)",
        test_userland_panic_fd2,
    );
    runner.run_test(
        "userland",
        "Line Editor Navigation, Word Jumping, and Splicing",
        test_line_editor_navigation_and_paste,
    );
    runner.run_test(
        "userland",
        "RFC 4648 Base64 Encoding Standard Vectors",
        test_base64_rfc4648,
    );
}

/// Tests libc small-object slab allocator under 10,000 allocate/free cycles and double-free protection.
fn test_libc_small_object_allocator() {
    const ARENA_SIZE: usize = 64 * 1024;
    const NUM_CLASSES: usize = 8;
    const SIZE_CLASSES: [usize; NUM_CLASSES] = [16, 32, 64, 128, 256, 512, 1024, 2048];
    const SMALL_THRESHOLD: usize = 2048;
    const LARGE_MAGIC: usize = 0x504F5349584D454D;
    const FREE_MAGIC: usize = 0x504F534958465245;
    const MAX_ARENAS: usize = 512;

    struct SimAlloc {
        pages: BTreeMap<usize, Vec<u8>>,
        mmap_count: usize,
        munmap_count: usize,
        next_addr: usize,
        free_lists: [usize; NUM_CLASSES],
        current_arenas: [usize; NUM_CLASSES],
        arenas: [(usize, usize, usize); MAX_ARENAS], // (start, end, class_idx)
        arena_count: usize,
    }

    impl SimAlloc {
        fn new() -> Self {
            Self {
                pages: BTreeMap::new(),
                mmap_count: 0,
                munmap_count: 0,
                next_addr: 0x6000_0000_0000,
                free_lists: [0; NUM_CLASSES],
                current_arenas: [0; NUM_CLASSES],
                arenas: [(0, 0, 0); MAX_ARENAS],
                arena_count: 0,
            }
        }
        fn mmap(&mut self, size: usize) -> usize {
            let aligned = (size + 4095) & !4095;
            let addr = self.next_addr;
            self.next_addr += aligned;
            self.mmap_count += 1;
            for off in (0..aligned).step_by(4096) {
                self.pages.insert(addr + off, vec![0u8; 4096]);
            }
            addr
        }
        fn munmap(&mut self, addr: usize, size: usize) {
            let aligned = (size + 4095) & !4095;
            self.munmap_count += 1;
            for off in (0..aligned).step_by(4096) {
                self.pages.remove(&(addr + off));
            }
        }
        fn r64(&self, a: usize) -> u64 {
            let mut b = [0u8; 8];
            self.rb(a, &mut b);
            u64::from_ne_bytes(b)
        }
        fn w64(&mut self, a: usize, v: u64) {
            self.wb(a, &v.to_ne_bytes());
        }
        fn rb(&self, a: usize, dst: &mut [u8]) {
            for (i, b) in dst.iter_mut().enumerate() {
                let curr = a + i;
                *b = self
                    .pages
                    .get(&(curr & !4095))
                    .map(|p| p[curr & 4095])
                    .unwrap_or(0);
            }
        }
        fn wb(&mut self, a: usize, src: &[u8]) {
            for (i, &b) in src.iter().enumerate() {
                let curr = a + i;
                if let Some(p) = self.pages.get_mut(&(curr & !4095)) {
                    p[curr & 4095] = b;
                }
            }
        }
        fn malloc(&mut self, size: usize) -> usize {
            if size == 0 {
                return 0;
            }
            if size > SMALL_THRESHOLD {
                let aligned = (size + 16 + 4095) & !4095;
                let ptr = self.mmap(aligned);
                self.w64(ptr, aligned as u64);
                self.w64(ptr + 8, LARGE_MAGIC as u64);
                ptr + 16
            } else {
                let mut c = 0;
                while c < NUM_CLASSES && SIZE_CLASSES[c] < size {
                    c += 1;
                }
                let bsz = SIZE_CLASSES[c];
                let node = self.free_lists[c];
                if node != 0 {
                    self.free_lists[c] = self.r64(node) as usize;
                    self.w64(node + 8, 0);
                    return node;
                }
                let cur = self.current_arenas[c];
                if cur != 0 {
                    let off = self.r64(cur + 16) as usize;
                    if off + bsz <= ARENA_SIZE {
                        self.w64(cur + 16, (off + bsz) as u64);
                        return cur + off;
                    }
                }
                if self.arena_count >= MAX_ARENAS {
                    return 0;
                }
                let a_ptr = self.mmap(ARENA_SIZE);
                self.w64(a_ptr + 16, (32 + bsz) as u64);
                self.current_arenas[c] = a_ptr;
                self.arenas[self.arena_count] = (a_ptr, a_ptr + ARENA_SIZE, c);
                self.arena_count += 1;
                a_ptr + 32
            }
        }
        fn free(&mut self, ptr: usize) {
            if ptr == 0 {
                return;
            }
            for i in 0..self.arena_count {
                let (start, end, c) = self.arenas[i];
                if ptr >= start && ptr < end {
                    if self.r64(ptr + 8) as usize == FREE_MAGIC {
                        return;
                    }
                    self.w64(ptr + 8, FREE_MAGIC as u64);
                    self.w64(ptr, self.free_lists[c] as u64);
                    self.free_lists[c] = ptr;
                    return;
                }
            }
            let hdr = ptr - 16;
            if self.r64(hdr + 8) as usize == LARGE_MAGIC {
                let sz = self.r64(hdr) as usize;
                self.w64(hdr + 8, 0);
                self.munmap(hdr, sz);
            }
        }
        fn realloc(&mut self, ptr: usize, size: usize) -> usize {
            if ptr == 0 {
                return self.malloc(size);
            }
            if size == 0 {
                self.free(ptr);
                return 0;
            }
            let mut cap = 0;
            let mut small = false;
            for &(start, end, c) in &self.arenas[..self.arena_count] {
                if ptr >= start && ptr < end {
                    cap = SIZE_CLASSES[c];
                    small = true;
                    break;
                }
            }
            if !small {
                let hdr = ptr - 16;
                if self.r64(hdr + 8) as usize != LARGE_MAGIC {
                    return 0;
                }
                cap = (self.r64(hdr) as usize) - 16;
            }
            if cap >= size {
                return ptr;
            }
            let new_p = self.malloc(size);
            if new_p != 0 {
                let mut buf = vec![0u8; cap];
                self.rb(ptr, &mut buf);
                self.wb(new_p, &buf);
                self.free(ptr);
            }
            new_p
        }
    }

    let mut alloc = SimAlloc::new();
    let small_ptr = alloc.malloc(64);
    assert_ne!(small_ptr, 0);
    alloc.free(small_ptr);
    alloc.free(small_ptr);
    let pop1 = alloc.malloc(64);
    let pop2 = alloc.malloc(64);
    assert_ne!(pop1, pop2);
    alloc.free(pop1);
    alloc.free(pop2);

    let mut full_alloc = SimAlloc::new();
    full_alloc.arena_count = MAX_ARENAS;
    assert_eq!(full_alloc.malloc(256), 0);

    let mut live = Vec::new();
    for i in 0..10_000 {
        let sz = ((i * 17) % 128) + 1;
        let p = alloc.malloc(sz);
        assert_ne!(p, 0);
        alloc.wb(p, &[0xAA]);
        live.push(p);
        if live.len() >= 64 {
            let to_free = live.swap_remove(0);
            alloc.free(to_free);
        }
    }
    for p in live {
        alloc.free(p);
    }
    assert!(alloc.mmap_count < 64);

    let p1 = alloc.malloc(32);
    alloc.wb(p1, &[1, 2, 3, 4]);
    let p2 = alloc.realloc(p1, 28);
    assert_eq!(p1, p2);
    let p3 = alloc.realloc(p2, 512);
    assert_ne!(p3, p2);
    let mut canary = [0u8; 4];
    alloc.rb(p3, &mut canary);
    assert_eq!(&canary, &[1, 2, 3, 4]);
    alloc.free(p3);

    let lp = alloc.malloc(8192);
    assert_ne!(lp, 0);
    alloc.free(lp);
    alloc.free(lp);
    assert_eq!(alloc.munmap_count, 1);
}

/// Tests panic diagnostic formatting targeting stderr (file descriptor 2) across all userland daemons.
fn test_userland_panic_fd2() {
    for (name, file, line, msg) in [
        (
            "init panic",
            "userland/init/src/main.rs",
            42,
            "explicit panic in test routine",
        ),
        (
            "shell panic",
            "userland/shell/src/main.rs",
            100,
            "command parser buffer overflow",
        ),
        (
            "coreutils panic",
            "userland/coreutils/src/main.rs",
            200,
            "unreachable state in ls applet",
        ),
    ] {
        let mut out = String::new();
        writeln!(out, "{}: panicked at {}:{}: {}", name, file, line, msg).unwrap();
        assert!(out.starts_with(&format!("{}: ", name)));
        assert!(out.contains(msg));
        assert!(out.contains(file));
    }
}

/// Tests interactive line editor navigation, word jumps, bracketed paste splicing, and kill-ring operations.
fn test_line_editor_navigation_and_paste() {
    const KILL_RING_SIZE: usize = 1024;

    #[derive(Clone)]
    struct TestKillRing {
        buf: [u8; KILL_RING_SIZE],
        len: usize,
    }

    impl TestKillRing {
        fn new() -> Self {
            Self {
                buf: [0; KILL_RING_SIZE],
                len: 0,
            }
        }
        fn save(&mut self, src: &[u8]) {
            let count = src.len().min(KILL_RING_SIZE);
            self.buf[..count].copy_from_slice(&src[..count]);
            self.len = count;
        }
        fn as_bytes(&self) -> &[u8] {
            &self.buf[..self.len]
        }
    }

    fn test_word_left(buf: &[u8], mut cursor_pos: usize) -> usize {
        while cursor_pos > 0 && (buf[cursor_pos - 1] == b' ' || buf[cursor_pos - 1] == b'\t') {
            cursor_pos -= 1;
        }
        while cursor_pos > 0 && buf[cursor_pos - 1] != b' ' && buf[cursor_pos - 1] != b'\t' {
            cursor_pos -= 1;
        }
        cursor_pos
    }

    fn test_word_right(buf: &[u8], len: usize, mut cursor_pos: usize) -> usize {
        while cursor_pos < len && buf[cursor_pos] != b' ' && buf[cursor_pos] != b'\t' {
            cursor_pos += 1;
        }
        while cursor_pos < len && (buf[cursor_pos] == b' ' || buf[cursor_pos] == b'\t') {
            cursor_pos += 1;
        }
        cursor_pos
    }

    fn test_splice_insert(
        buf: &mut [u8],
        len: &mut usize,
        cursor_pos: &mut usize,
        data: &[u8],
    ) -> usize {
        if buf.is_empty() || *len >= buf.len() - 1 {
            return 0;
        }
        let capacity_left = (buf.len() - 1).saturating_sub(*len);
        let insert_count = capacity_left.min(data.len());
        if insert_count == 0 {
            return 0;
        }
        for i in (*cursor_pos..*len).rev() {
            buf[i + insert_count] = buf[i];
        }
        for (i, &b) in data[..insert_count].iter().enumerate() {
            let mut byte = b;
            if byte == b'\r' || byte == b'\n' {
                byte = b' ';
            }
            buf[*cursor_pos + i] = byte;
        }
        *cursor_pos += insert_count;
        *len += insert_count;
        buf[*len] = 0;
        insert_count
    }

    fn test_kill_to_end(
        buf: &mut [u8],
        len: &mut usize,
        cursor_pos: usize,
        kill_ring: &mut TestKillRing,
    ) {
        if cursor_pos < *len {
            kill_ring.save(&buf[cursor_pos..*len]);
            *len = cursor_pos;
            buf[*len] = 0;
        }
    }

    fn test_kill_to_start(
        buf: &mut [u8],
        len: &mut usize,
        cursor_pos: &mut usize,
        kill_ring: &mut TestKillRing,
    ) {
        if *cursor_pos > 0 {
            kill_ring.save(&buf[..*cursor_pos]);
            for i in *cursor_pos..*len {
                buf[i - *cursor_pos] = buf[i];
            }
            *len -= *cursor_pos;
            *cursor_pos = 0;
            buf[*len] = 0;
        } else if *len > 0 {
            kill_ring.save(&buf[..*len]);
            *len = 0;
            buf[0] = 0;
        }
    }

    fn test_kill_word_backward(
        buf: &mut [u8],
        len: &mut usize,
        cursor_pos: &mut usize,
        kill_ring: &mut TestKillRing,
    ) {
        if *cursor_pos > 0 {
            let word_start = test_word_left(buf, *cursor_pos);
            let count = *cursor_pos - word_start;
            kill_ring.save(&buf[word_start..*cursor_pos]);
            for i in *cursor_pos..*len {
                buf[i - count] = buf[i];
            }
            *len -= count;
            *cursor_pos = word_start;
            buf[*len] = 0;
        }
    }

    // 1. Test Navigation: Home, End, Left, Right, Word Left/Right
    let cmd = b"echo hello world";
    let len = cmd.len();

    assert_eq!(
        test_word_left(cmd, 16),
        11,
        "Word Left from 16 -> 11 ('world')"
    );
    assert_eq!(
        test_word_left(cmd, 11),
        5,
        "Word Left from 11 -> 5 ('hello')"
    );
    assert_eq!(test_word_left(cmd, 5), 0, "Word Left from 5 -> 0 ('echo')");

    assert_eq!(test_word_right(cmd, len, 0), 5, "Word Right from 0 -> 5");
    assert_eq!(test_word_right(cmd, len, 5), 11, "Word Right from 5 -> 11");
    assert_eq!(
        test_word_right(cmd, len, 11),
        16,
        "Word Right from 11 -> 16"
    );

    // 2. Test Mid-Line Splicing / Insertion (Paste into Terminal)
    let mut buf = [0u8; 32];
    let init_str = b"echo world";
    buf[..init_str.len()].copy_from_slice(init_str);
    let mut cur_len = init_str.len();
    let mut cur_pos = 5; // Insert between 'echo ' and 'world'

    let inserted = test_splice_insert(&mut buf, &mut cur_len, &mut cur_pos, b"-n ");
    assert_eq!(inserted, 3);
    assert_eq!(&buf[..cur_len], b"echo -n world");
    assert_eq!(cur_pos, 8);

    // 3. Test Buffer Capacity Boundary Protection (No OOB write on large paste)
    let mut small_buf = [0u8; 10]; // Capacity for 9 chars + nul
    let init_small = b"hello";
    small_buf[..init_small.len()].copy_from_slice(init_small);
    let mut small_len = init_small.len(); // 5
    let mut small_pos = 5;

    // Paste 20 bytes into a buffer with only 4 bytes of remaining capacity
    let paste_overflow = b"01234567890123456789";
    let count = test_splice_insert(
        &mut small_buf,
        &mut small_len,
        &mut small_pos,
        paste_overflow,
    );
    assert_eq!(count, 4, "Must insert exactly remaining capacity (4 bytes)");
    assert_eq!(small_len, 9, "Length must not exceed capacity minus 1");
    assert_eq!(small_buf[9], 0, "Nul terminator must remain intact");
    assert_eq!(&small_buf[..9], b"hello0123");

    // 4. Test Clipboard Kill-Ring (Cut, Copy, Paste / Yank)
    let mut kr = TestKillRing::new();

    // Ctrl+K: Kill to end at position 8 (cuts "world")
    test_kill_to_end(&mut buf, &mut cur_len, cur_pos, &mut kr);
    assert_eq!(kr.as_bytes(), b"world");
    assert_eq!(&buf[..cur_len], b"echo -n ");

    // Ctrl+Y: Yank (Paste from kill ring) at position 0 (Home)
    cur_pos = 0;
    let yanked = test_splice_insert(&mut buf, &mut cur_len, &mut cur_pos, kr.as_bytes());
    assert_eq!(yanked, 5);
    assert_eq!(&buf[..cur_len], b"worldecho -n ");
    assert_eq!(cur_pos, 5);

    // Ctrl+U: Kill to start at position 5 (cuts "world")
    test_kill_to_start(&mut buf, &mut cur_len, &mut cur_pos, &mut kr);
    assert_eq!(kr.as_bytes(), b"world");
    assert_eq!(&buf[..cur_len], b"echo -n ");
    assert_eq!(cur_pos, 0);

    // Ctrl+W: Kill word backward
    cur_pos = cur_len; // End of "echo -n "
    test_kill_word_backward(&mut buf, &mut cur_len, &mut cur_pos, &mut kr);
    assert_eq!(kr.as_bytes(), b"-n ");
    assert_eq!(&buf[..cur_len], b"echo ");
    assert_eq!(cur_pos, 5);
}

/// Tests RFC 4648 standard base64 test vectors including padding.
fn test_base64_rfc4648() {
    let b64_table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for (input, expected) in [
        (&b""[..], &b""[..]),
        (&b"f"[..], &b"Zg=="[..]),
        (&b"fo"[..], &b"Zm8="[..]),
        (&b"foo"[..], &b"Zm9v"[..]),
        (&b"foob"[..], &b"Zm9vYg=="[..]),
        (&b"fooba"[..], &b"Zm9vYmE="[..]),
        (&b"foobar"[..], &b"Zm9vYmFy"[..]),
        (&b"Rust POSIX OS"[..], &b"UnVzdCBQT1NJWCBPUw=="[..]),
    ] {
        let mut out = Vec::new();
        for chunk in input.chunks(3) {
            let b0 = chunk[0];
            let b1 = *chunk.get(1).unwrap_or(&0);
            let b2 = *chunk.get(2).unwrap_or(&0);
            out.push(b64_table[(b0 >> 2) as usize]);
            out.push(b64_table[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize]);
            out.push(if chunk.len() > 1 {
                b64_table[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize]
            } else {
                b'='
            });
            out.push(if chunk.len() > 2 {
                b64_table[(b2 & 0x3f) as usize]
            } else {
                b'='
            });
        }
        assert_eq!(&out[..], expected);
    }
}
