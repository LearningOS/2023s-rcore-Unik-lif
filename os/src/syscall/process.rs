//! Process management syscalls
use crate::{
    config::MAX_SYSCALL_NUM,
    task::{
        exit_current_and_run_next, pass_syscall_info, pass_task_status,
        suspend_current_and_run_next, SyscallInfo, TaskStatus,
    },
    timer::{get_time_ms, get_time_us},
};

#[repr(C)]
#[derive(Debug)]
/// Time Value.
pub struct TimeVal {
    /// second of our time.
    pub sec: usize,
    /// usecond of our time.
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
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
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
