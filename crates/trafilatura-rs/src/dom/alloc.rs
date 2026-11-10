//! libxml2's allocator (installed with `xmlMemSetup`): while a `Ctx` is alive on a thread, every libxml2
//! allocation comes from a per-thread bump arena, `free` of such a block is a no-op, and dropping the last `Ctx`
//! releases everything at once. Tree teardown and most malloc/free cost disappear.
//!
//! Every block carries a 16-byte header (size, tag) so `free`/`realloc` know where it came from; an unknown tag
//! aborts, which turns a dangling libxml2 pointer into an immediate crash instead of silent corruption.
//! Long-lived libxml2 objects (compiled XPath) must be created inside `outside_arena`.
//! The per-thread state has no destructor (so every access is a plain TLS load); a thread that exits keeps up to
//! `RETAIN` bytes of chunks allocated.
use std::alloc::{alloc, dealloc, realloc as sys_realloc, Layout};
use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::os::raw::{c_char, c_void};
use std::ptr;

const HDR: usize = 16;
const ALIGN: usize = 16;
const TAG_SYS: usize = 0x5359_5354_4147_0001;
const TAG_ARENA: usize = 0x4152_4e41_5441_0002;
const CHUNK: usize = 1 << 20;
/// Chunk bytes kept for reuse after a reset; larger arenas give memory back.
const RETAIN: usize = 32 << 20;

struct State {
    chunks: ManuallyDrop<Vec<(*mut u8, usize)>>,
    /// Index of the chunk `cur..end` points into (chunks after it are free, retained from earlier resets).
    idx: usize,
    cur: *mut u8,
    end: *mut u8,
    /// Number of live `Ctx` on this thread; the arena serves allocations while > 0 and not paused.
    depth: usize,
    paused: bool,
    /// Bumped at every reset: arena addresses (a dict's, say) repeat across resets, so caches keyed by an
    /// address must also check the generation.
    generation: u64,
}

thread_local! {
    static STATE: UnsafeCell<State> = const {
        UnsafeCell::new(State {
            chunks: ManuallyDrop::new(Vec::new()),
            idx: 0,
            cur: ptr::null_mut(),
            end: ptr::null_mut(),
            depth: 0,
            paused: false,
            generation: 0,
        })
    };
}

fn state() -> *mut State {
    STATE.with(UnsafeCell::get)
}

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size + HDR, ALIGN).expect("allocation size")
}

fn chunk_layout(cap: usize) -> Layout {
    Layout::from_size_align(cap, ALIGN).expect("chunk")
}

unsafe fn write_header(base: *mut u8, size: usize, tag: usize) -> *mut c_void {
    (base as *mut usize).write(size);
    (base as *mut usize).add(1).write(tag);
    base.add(HDR) as *mut c_void
}

unsafe fn header(p: *mut c_void) -> (*mut u8, usize, usize) {
    let base = (p as *mut u8).sub(HDR);
    (base, (base as *const usize).read(), (base as *const usize).add(1).read())
}

unsafe fn arena_active(s: *mut State) -> bool {
    (*s).depth > 0 && !(*s).paused
}

unsafe fn arena_alloc(s: &mut State, size: usize) -> *mut c_void {
    let need = (size + HDR + ALIGN - 1) & !(ALIGN - 1);
    if s.cur.is_null() || (s.end as usize) - (s.cur as usize) < need {
        let next = if s.cur.is_null() { 0 } else { s.idx + 1 };
        let (chunk, cap) = match s.chunks.get(next) {
            Some(&(c, cap)) if cap >= need => (c, cap),
            _ => {
                let cap = need.max(CHUNK);
                let c = alloc(chunk_layout(cap));
                assert!(!c.is_null(), "out of memory");
                let at = next.min(s.chunks.len());
                s.chunks.insert(at, (c, cap));
                (c, cap)
            }
        };
        s.idx = next.min(s.chunks.len() - 1);
        s.cur = chunk;
        s.end = chunk.add(cap);
    }
    let base = s.cur;
    s.cur = s.cur.add(need);
    write_header(base, size, TAG_ARENA)
}

unsafe fn sys_alloc(size: usize) -> *mut c_void {
    let base = alloc(layout(size));
    if base.is_null() {
        return ptr::null_mut();
    }
    write_header(base, size, TAG_SYS)
}

unsafe extern "C" fn x_malloc(size: usize) -> *mut c_void {
    let s = state();
    if arena_active(s) {
        arena_alloc(&mut *s, size)
    } else {
        sys_alloc(size)
    }
}

unsafe extern "C" fn x_free(p: *mut c_void) {
    if p.is_null() {
        return;
    }
    match header(p) {
        (base, size, TAG_SYS) => dealloc(base, layout(size)),
        (_, _, TAG_ARENA) => {}
        _ => std::process::abort(),
    }
}

unsafe extern "C" fn x_realloc(p: *mut c_void, size: usize) -> *mut c_void {
    if p.is_null() {
        return x_malloc(size);
    }
    let s = state();
    // A block keeps its kind: a long-lived system block must never migrate into the arena.
    match header(p) {
        (base, old, TAG_SYS) => {
            let nb = sys_realloc(base, layout(old), size + HDR);
            if nb.is_null() {
                return ptr::null_mut();
            }
            write_header(nb, size, TAG_SYS)
        }
        (_, old, TAG_ARENA) if arena_active(s) => {
            let q = arena_alloc(&mut *s, size);
            ptr::copy_nonoverlapping(p as *const u8, q as *mut u8, old.min(size));
            q
        }
        _ => std::process::abort(),
    }
}

unsafe extern "C" fn x_strdup(src: *const c_char) -> *mut c_char {
    let len = std::ffi::CStr::from_ptr(src).to_bytes_with_nul().len();
    let q = x_malloc(len) as *mut c_char;
    if !q.is_null() {
        ptr::copy_nonoverlapping(src, q, len);
    }
    q
}

/// Installs the allocator; must run before any other libxml2 call.
pub(crate) unsafe fn install() {
    let rc = libxml2_sys::xmlMemSetup(Some(x_free), Some(x_malloc), Some(x_realloc), Some(x_strdup));
    assert_eq!(rc, 0, "xmlMemSetup failed");
}

pub(crate) fn enter() {
    unsafe { (*state()).depth += 1 };
}

/// Leaves one `Ctx`; the last one out drops libxml2's copy of the last error (it may point into the arena) and
/// resets the arena.
pub(crate) fn leave() {
    unsafe {
        let s = &mut *state();
        s.depth -= 1;
        if s.depth > 0 {
            return;
        }
        libxml2_sys::xmlResetLastError();
        s.generation += 1;
        let mut kept = 0;
        s.chunks.retain(|&(chunk, cap)| {
            kept += cap;
            if kept > RETAIN {
                dealloc(chunk, chunk_layout(cap));
                return false;
            }
            true
        });
        s.idx = 0;
        (s.cur, s.end) = match s.chunks.first() {
            Some(&(chunk, cap)) => (chunk, chunk.add(cap)),
            None => (ptr::null_mut(), ptr::null_mut()),
        };
    }
}

pub(crate) fn generation() -> u64 {
    unsafe { (*state()).generation }
}

/// Runs `f` with the arena bypassed, for libxml2 objects that outlive the current `Ctx`.
pub(crate) fn outside_arena<R>(f: impl FnOnce() -> R) -> R {
    let s = state();
    let was = unsafe { std::mem::replace(&mut (*s).paused, true) };
    let r = f();
    unsafe { (*s).paused = was };
    r
}
