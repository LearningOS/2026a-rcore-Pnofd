//! Process management syscalls
use crate::config::PAGE_SIZE;
use crate::mm::{
    translated_byte_buffer, MapPermission, PageTable, PageTableEntry, VirtAddr,
};
use crate::task::{
    change_program_brk, current_user_token, exit_current_and_run_next, get_syscall_count, mmap,
    suspend_current_and_run_next, unmap,
};
use crate::timer::get_time_us;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// Look up the page table entry backing user address `va`.
fn translate_user_va(token: usize, va: usize) -> Option<PageTableEntry> {
    // SV39 only defines canonical addresses: bits 63..39 must all equal bit 38.
    let high = va >> 39;
    if high != 0 && high != (1usize << 25) - 1 {
        return None;
    }
    PageTable::from_token(token)
        .translate(VirtAddr::from(va).floor())
        .filter(|pte| pte.is_valid())
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    let time_val = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    let src = unsafe {
        core::slice::from_raw_parts(
            &time_val as *const TimeVal as *const u8,
            core::mem::size_of::<TimeVal>(),
        )
    };
    let mut copied = 0;
    for page in translated_byte_buffer(current_user_token(), ts as *const u8, src.len()) {
        let len = page.len();
        page.copy_from_slice(&src[copied..copied + len]);
        copied += len;
    }
    0
}

/// Trace the current task's memory or syscall history.
///
/// - `trace_request == 0`: read one byte from user address `id`.
/// - `trace_request == 1`: write the lowest byte of `data` to user address `id`.
/// - `trace_request == 2`: return how many times syscall `id` has been called.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    let token = current_user_token();
    match trace_request {
        0 => match translate_user_va(token, id) {
            Some(pte) if pte.readable() => {
                let ppn = pte.ppn();
                ppn.get_bytes_array()[id & (PAGE_SIZE - 1)] as isize
            }
            _ => -1,
        },
        1 => match translate_user_va(token, id) {
            Some(pte) if pte.writable() => {
                let ppn = pte.ppn();
                ppn.get_bytes_array()[id & (PAGE_SIZE - 1)] = data as u8;
                0
            }
            _ => -1,
        },
        2 => get_syscall_count(id) as isize,
        _ => -1,
    }
}

/// Round `len` up to a whole number of pages.
fn round_up_to_page(len: usize) -> Option<usize> {
    len.checked_add(PAGE_SIZE - 1)
        .map(|len| len & !(PAGE_SIZE - 1))
}

/// Map anonymous memory at `start` with the user supplied protection bits.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap");
    if start & (PAGE_SIZE - 1) != 0 {
        return -1;
    }
    if prot & !0x7 != 0 || prot & 0x7 == 0 {
        return -1;
    }
    let Some(map_len) = round_up_to_page(len) else {
        return -1;
    };
    let mut permission = MapPermission::U;
    if prot & 0x1 != 0 {
        permission |= MapPermission::R;
    }
    if prot & 0x2 != 0 {
        permission |= MapPermission::W;
    }
    if prot & 0x4 != 0 {
        permission |= MapPermission::X;
    }
    if mmap(start, map_len, permission) {
        0
    } else {
        -1
    }
}

/// Remove the mapping of `len` bytes starting at `start`.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    if start & (PAGE_SIZE - 1) != 0 {
        return -1;
    }
    let Some(map_len) = round_up_to_page(len) else {
        return -1;
    };
    if unmap(start, map_len) {
        0
    } else {
        -1
    }
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
