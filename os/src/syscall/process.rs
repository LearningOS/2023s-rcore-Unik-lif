//! Process management syscalls


use crate::{
    config::MAX_SYSCALL_NUM,
    mm::translated_va2pa,
    task::{
        change_program_brk, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus,
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
#[derive(Copy, Clone)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
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

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
     
    // first get the time here.
    let us = get_time_us();
    let sec = us / 1_000_000;
    let usec = us % 1_000_000;
    // translate virtual address to physical address.
    // 1. get the current page_table.
    // 2. find the ppn.
    // 3. get the exact physical address, write sec and usec here into the memory.
    let sec_va = _ts as usize;
    let usec_va = _ts as usize + 8;
    let sec_pa = translated_va2pa(current_user_token(), sec_va) as *mut usize;
    let usec_pa = translated_va2pa(current_user_token(), usec_va) as *mut usize;
    unsafe {
        *sec_pa = sec;
        *usec_pa = usec;
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");
    -1
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    -1
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    -1
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
/*
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    let status: TaskStatus = pass_task_status();
    let sys_info: SyscallInfo = pass_syscall_info();
    if status != TaskStatus::Running {
        return -1;
    }
    unsafe {
        (*_ti).status = TaskStatus::Running;
        (*_ti).syscall_times = sys_info.syscall_times;
        (*_ti).time = get_time_ms() - sys_info.time;
    }
    0
}
*/