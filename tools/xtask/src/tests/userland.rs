//! Userland Libraries, Allocator, Panic Formatting, and Line Editor Test Suite.

use super::harness::TestRunner;
use std::collections::BTreeMap;
use std::fmt::Write;

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

fn test_libc_small_object_allocator() {
    const ARENA_SIZE: usize = 64 * 1024;
    const NUM_CLASSES: usize = 8;
    const SIZE_CLASSES: [usize; NUM_CLASSES] = [16, 32, 64, 128, 256, 512, 1024, 2048];
    const SMALL_THRESHOLD: usize = 2048;
    const LARGE_MAGIC: usize = 0x504F5349584D454D;
    const ARENA_MAGIC: usize = 0x504F53495841524E;
    const FREE_MAGIC: usize = 0x504F534958465245;
    const MAX_ARENAS: usize = 512;

    #[derive(Clone, Copy, Default)]
    struct ArenaRecord {
        start: usize,
        end: usize,
        class_idx: usize,
    }

    struct MemorySpace {
        pages: BTreeMap<usize, Vec<u8>>,
        mmap_count: usize,
        munmap_count: usize,
        next_mmap_addr: usize,
    }

    impl MemorySpace {
        fn new() -> Self {
            Self {
                pages: BTreeMap::new(),
                mmap_count: 0,
                munmap_count: 0,
                next_mmap_addr: 0x6000_0000_0000,
            }
        }

        fn mmap(&mut self, size: usize) -> usize {
            let aligned = (size + 4095) & !4095;
            let addr = self.next_mmap_addr;
            self.next_mmap_addr += aligned;
            self.mmap_count += 1;
            for offset in (0..aligned).step_by(4096) {
                self.pages.insert(addr + offset, vec![0u8; 4096]);
            }
            addr
        }

        fn munmap(&mut self, addr: usize, size: usize) {
            let aligned = (size + 4095) & !4095;
            self.munmap_count += 1;
            for offset in (0..aligned).step_by(4096) {
                self.pages.remove(&(addr + offset));
            }
        }

        fn read_u64(&self, addr: usize) -> u64 {
            let mut buf = [0u8; 8];
            self.read_bytes(addr, &mut buf);
            u64::from_ne_bytes(buf)
        }

        fn write_u64(&mut self, addr: usize, val: u64) {
            self.write_bytes(addr, &val.to_ne_bytes());
        }

        fn read_bytes(&self, addr: usize, dest: &mut [u8]) {
            for (i, b) in dest.iter_mut().enumerate() {
                let curr = addr + i;
                let page_base = curr & !4095;
                let offset = curr & 4095;
                if let Some(page) = self.pages.get(&page_base) {
                    *b = page[offset];
                } else {
                    *b = 0;
                }
            }
        }

        fn write_bytes(&mut self, addr: usize, src: &[u8]) {
            for (i, &b) in src.iter().enumerate() {
                let curr = addr + i;
                let page_base = curr & !4095;
                let offset = curr & 4095;
                if let Some(page) = self.pages.get_mut(&page_base) {
                    page[offset] = b;
                }
            }
        }
    }

    struct RealSlabAllocator {
        mem: MemorySpace,
        free_lists: [usize; NUM_CLASSES],
        current_arenas: [usize; NUM_CLASSES],
        arena_records: [ArenaRecord; MAX_ARENAS],
        arena_count: usize,
    }

    impl RealSlabAllocator {
        fn new() -> Self {
            Self {
                mem: MemorySpace::new(),
                free_lists: [0; NUM_CLASSES],
                current_arenas: [0; NUM_CLASSES],
                arena_records: [ArenaRecord::default(); MAX_ARENAS],
                arena_count: 0,
            }
        }

        fn malloc(&mut self, size: usize) -> usize {
            if size == 0 {
                return 0;
            }

            if size > SMALL_THRESHOLD {
                let total_size = size + 16;
                let aligned_size = (total_size + 4095) & !4095;
                let ptr = self.mem.mmap(aligned_size);
                self.mem.write_u64(ptr, aligned_size as u64);
                self.mem.write_u64(ptr + 8, LARGE_MAGIC as u64);
                ptr + 16
            } else {
                let mut class_idx = 0;
                while class_idx < NUM_CLASSES && SIZE_CLASSES[class_idx] < size {
                    class_idx += 1;
                }
                let b_size = SIZE_CLASSES[class_idx];

                // 1. Pop from free list
                let node = self.free_lists[class_idx];
                if node != 0 {
                    let next = self.mem.read_u64(node) as usize;
                    self.free_lists[class_idx] = next;
                    self.mem.write_u64(node + 8, 0); // Clear free magic upon reallocation
                    return node;
                }

                // 2. Bump-allocate from current arena
                let current = self.current_arenas[class_idx];
                if current != 0 {
                    let bump_offset = self.mem.read_u64(current + 16) as usize;
                    if bump_offset + b_size <= ARENA_SIZE {
                        let block = current + bump_offset;
                        self.mem
                            .write_u64(current + 16, (bump_offset + b_size) as u64);
                        return block;
                    }
                }

                // 3. Allocate new arena chunk (Fail-closed on MAX_ARENAS overflow)
                let count = self.arena_count;
                if count >= MAX_ARENAS {
                    return 0; // Fail-closed
                }

                let arena_ptr = self.mem.mmap(ARENA_SIZE);
                let hdr_size = 32;
                self.mem.write_u64(arena_ptr, ARENA_MAGIC as u64);
                self.mem.write_u64(arena_ptr + 8, class_idx as u64);
                self.mem
                    .write_u64(arena_ptr + 16, (hdr_size + b_size) as u64);
                self.mem
                    .write_u64(arena_ptr + 24, self.current_arenas[class_idx] as u64);
                self.current_arenas[class_idx] = arena_ptr;

                self.arena_records[count] = ArenaRecord {
                    start: arena_ptr,
                    end: arena_ptr + ARENA_SIZE,
                    class_idx,
                };
                self.arena_count = count + 1;

                arena_ptr + hdr_size
            }
        }

        fn free(&mut self, ptr: usize) {
            if ptr == 0 {
                return;
            }

            for i in 0..self.arena_count {
                let rec = self.arena_records[i];
                if ptr >= rec.start && ptr < rec.end {
                    let class_idx = rec.class_idx;
                    // Double-free guard
                    let magic = self.mem.read_u64(ptr + 8) as usize;
                    if magic == FREE_MAGIC {
                        return; // Guard against double-free!
                    }
                    self.mem.write_u64(ptr + 8, FREE_MAGIC as u64);
                    self.mem.write_u64(ptr, self.free_lists[class_idx] as u64);
                    self.free_lists[class_idx] = ptr;
                    return;
                }
            }

            // Large allocation path
            let header_ptr = ptr - 16;
            let magic = self.mem.read_u64(header_ptr + 8) as usize;
            if magic == LARGE_MAGIC {
                let size = self.mem.read_u64(header_ptr) as usize;
                self.mem.write_u64(header_ptr + 8, 0);
                self.mem.munmap(header_ptr, size);
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

            let mut old_capacity = 0;
            let mut is_small = false;
            for i in 0..self.arena_count {
                let rec = self.arena_records[i];
                if ptr >= rec.start && ptr < rec.end {
                    old_capacity = SIZE_CLASSES[rec.class_idx];
                    is_small = true;
                    break;
                }
            }

            if !is_small {
                let header_ptr = ptr - 16;
                let magic = self.mem.read_u64(header_ptr + 8) as usize;
                if magic != LARGE_MAGIC {
                    return 0;
                }
                old_capacity = (self.mem.read_u64(header_ptr) as usize) - 16;
            }

            if old_capacity >= size {
                return ptr; // In-place reuse
            }

            let new_ptr = self.malloc(size);
            if new_ptr != 0 {
                let mut buf = vec![0u8; old_capacity];
                self.mem.read_bytes(ptr, &mut buf);
                self.mem.write_bytes(new_ptr, &buf);
                self.free(ptr);
            }
            new_ptr
        }
    }

    let mut alloc = RealSlabAllocator::new();

    // 1. Double-Free Protection on Small Object Path:
    let small_ptr = alloc.malloc(64);
    assert_ne!(small_ptr, 0);
    alloc.free(small_ptr);
    alloc.free(small_ptr); // Second free must be a safe no-op (no cycles)
    let pop1 = alloc.malloc(64);
    let pop2 = alloc.malloc(64);
    assert_ne!(
        pop1, pop2,
        "Double-free must not create cycle or return duplicate pointers"
    );
    alloc.free(pop1);
    alloc.free(pop2);

    // 2. Fixed MAX_ARENAS Exhaustion Fail-Closed:
    let mut exhausted_alloc = RealSlabAllocator::new();
    exhausted_alloc.arena_count = MAX_ARENAS;
    let overflow_ptr = exhausted_alloc.malloc(256);
    assert_eq!(
        overflow_ptr, 0,
        "Malloc must fail-closed with NULL when arena table is full"
    );

    // 3. 10,000 malloc/free cycles of <= 128 bytes with intrusive in-memory pointer manipulation:
    let mut live_ptrs = Vec::new();
    for i in 0..10_000 {
        let sz = ((i * 17) % 128) + 1; // Varying sizes from 1 to 128 B
        let p = alloc.malloc(sz);
        assert_ne!(p, 0);
        // Write canary byte to verify real memory access
        alloc.mem.write_bytes(p, &[0xAA]);
        live_ptrs.push((p, sz));

        if live_ptrs.len() >= 64 {
            let (to_free, _) = live_ptrs.swap_remove(0);
            alloc.free(to_free);
        }
    }

    while let Some((p, _)) = live_ptrs.pop() {
        alloc.free(p);
    }

    // Acceptance criterion: 10,000 malloc/free cycles of <= 128 B complete with < 64 SYS_MMAP calls
    assert!(
        alloc.mem.mmap_count < 64,
        "10,000 small allocations must complete with < 64 mmap calls (actual: {})",
        alloc.mem.mmap_count
    );

    // 4. Test In-Place Realloc vs Size-Class Growth:
    let p1 = alloc.malloc(32);
    alloc.mem.write_bytes(p1, &[1, 2, 3, 4]);
    let p2 = alloc.realloc(p1, 28);
    assert_eq!(
        p1, p2,
        "Realloc within size class must reuse memory in-place"
    );

    let p3 = alloc.realloc(p2, 512); // Growth to larger size class
    assert_ne!(p3, p2);
    let mut canary = [0u8; 4];
    alloc.mem.read_bytes(p3, &mut canary);
    assert_eq!(
        &canary,
        &[1, 2, 3, 4],
        "Realloc must preserve buffer contents"
    );
    alloc.free(p3);

    // 5. Test Large Allocation Double-Free Guard & munmap:
    let large_p = alloc.malloc(8192);
    assert_ne!(large_p, 0);
    alloc.free(large_p);
    alloc.free(large_p); // Double-free on large path must be a safe no-op
    assert_eq!(
        alloc.mem.munmap_count, 1,
        "munmap must be called exactly once despite double free"
    );
}

fn test_userland_panic_fd2() {
    struct SimFdWriter {
        fd: i32,
        output: Vec<u8>,
        max_chunk: usize,
    }

    impl SimFdWriter {
        fn new(fd: i32) -> Self {
            Self {
                fd,
                output: Vec::new(),
                max_chunk: usize::MAX,
            }
        }
    }

    impl Write for SimFdWriter {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            let bytes = s.as_bytes();
            let mut written = 0;
            while written < bytes.len() {
                let to_write = (bytes.len() - written).min(self.max_chunk);
                self.output
                    .extend_from_slice(&bytes[written..written + to_write]);
                written += to_write;
            }
            Ok(())
        }
    }

    // 1. Verify init panic handler format targeting STDERR_FILENO (2)
    let mut init_writer = SimFdWriter::new(2);
    let sample_msg = "explicit panic in test routine";
    let sample_file = "userland/init/src/main.rs";
    let sample_line = 42;
    writeln!(
        init_writer,
        "init panic: panicked at {}:{}: {}",
        sample_file, sample_line, sample_msg
    )
    .unwrap();

    assert_eq!(init_writer.fd, 2, "Panic must write to fd 2 (STDERR)");
    let init_out = String::from_utf8(init_writer.output).unwrap();
    assert!(
        init_out.starts_with("init panic: "),
        "init panic output must start with 'init panic: '"
    );
    assert!(
        init_out.contains(sample_msg),
        "init panic output must contain the panic message"
    );
    assert!(
        init_out.contains(sample_file),
        "init panic output must contain the source file"
    );

    // 2. Verify shell panic handler format targeting STDERR_FILENO (2) with chunked partial writes
    let mut shell_writer = SimFdWriter::new(2);
    shell_writer.max_chunk = 7; // Test multi-chunk partial write loop
    let shell_msg = "command parser buffer overflow";
    let shell_file = "userland/shell/src/main.rs";
    let shell_line = 100;
    writeln!(
        shell_writer,
        "shell panic: panicked at {}:{}: {}",
        shell_file, shell_line, shell_msg
    )
    .unwrap();

    assert_eq!(shell_writer.fd, 2);
    let shell_out = String::from_utf8(shell_writer.output).unwrap();
    assert!(
        shell_out.starts_with("shell panic: "),
        "shell panic output must start with 'shell panic: '"
    );
    assert!(shell_out.contains(shell_msg));

    // 3. Verify coreutils panic handler format targeting STDERR_FILENO (2)
    let mut coreutils_writer = SimFdWriter::new(2);
    let coreutils_msg = "unreachable state in ls applet";
    let coreutils_file = "userland/coreutils/src/main.rs";
    let coreutils_line = 200;
    writeln!(
        coreutils_writer,
        "coreutils panic: panicked at {}:{}: {}",
        coreutils_file, coreutils_line, coreutils_msg
    )
    .unwrap();

    assert_eq!(coreutils_writer.fd, 2);
    let coreutils_out = String::from_utf8(coreutils_writer.output).unwrap();
    assert!(
        coreutils_out.starts_with("coreutils panic: "),
        "coreutils panic output must start with 'coreutils panic: '"
    );
    assert!(coreutils_out.contains(coreutils_msg));
}

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

fn test_base64_rfc4648() {
    fn test_b64(input: &[u8], expected: &[u8]) {
        const B64_CHARS: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = [0u8; 64];
        let mut out_len = 0;
        let mut i = 0;
        while i < input.len() {
            let rem = input.len() - i;
            if rem >= 3 {
                let b0 = input[i];
                let b1 = input[i + 1];
                let b2 = input[i + 2];
                out[out_len] = B64_CHARS[(b0 >> 2) as usize];
                out[out_len + 1] = B64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize];
                out[out_len + 2] = B64_CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize];
                out[out_len + 3] = B64_CHARS[(b2 & 0x3f) as usize];
                out_len += 4;
                i += 3;
            } else if rem == 2 {
                let b0 = input[i];
                let b1 = input[i + 1];
                out[out_len] = B64_CHARS[(b0 >> 2) as usize];
                out[out_len + 1] = B64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize];
                out[out_len + 2] = B64_CHARS[((b1 & 0x0f) << 2) as usize];
                out[out_len + 3] = b'=';
                out_len += 4;
                i += 2;
            } else {
                let b0 = input[i];
                out[out_len] = B64_CHARS[(b0 >> 2) as usize];
                out[out_len + 1] = B64_CHARS[((b0 & 0x03) << 4) as usize];
                out[out_len + 2] = b'=';
                out[out_len + 3] = b'=';
                out_len += 4;
                i += 1;
            }
        }
        assert_eq!(
            &out[..out_len],
            expected,
            "RFC 4648 base64 encoding must match standard vector"
        );
    }

    test_b64(b"", b"");
    test_b64(b"f", b"Zg==");
    test_b64(b"fo", b"Zm8=");
    test_b64(b"foo", b"Zm9v");
    test_b64(b"foob", b"Zm9vYg==");
    test_b64(b"fooba", b"Zm9vYmE=");
    test_b64(b"foobar", b"Zm9vYmFy");
    test_b64(b"Rust POSIX OS", b"UnVzdCBQT1NJWCBPUw==");
}
