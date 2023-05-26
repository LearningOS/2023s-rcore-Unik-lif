//! Process management syscalls
//!

use alloc::sync::Arc;

use crate::{
    config::{MAX_SYSCALL_NUM, BIG_STRIDE},
    fs::{open_file, OpenFlags},
    mm::{translated_refmut, translated_str, VirtAddr, MapPermission},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus, TaskControlBlock, pass_task_status, SyscallInfo, pass_syscall_info, push_current_area, release_current_area
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
/// TimeVal: store time info.
pub struct TimeVal {
    /// sec part.
    pub sec: usize,
    /// usec part.
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
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
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}
/// fork a process.
pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}
/// exec a new process.
pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    // first get the time here.
    let us = get_time_us();
    let sec = us / 1_000_000;
    let usec = us % 1_000_000;

    let sec_va = _ts as usize;
    let usec_va = _ts as usize + 8;
    let sec_pa = translated_refmut(token, sec_va as *mut usize) as *mut usize;
    let usec_pa = translated_refmut(token, usec_va  as *mut usize) as *mut usize;
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
    trace!(
        "kernel:pid[{}] sys_task_info NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // first get taskinfo here.
    let token = current_user_token();
    let status: TaskStatus = pass_task_status();
    let sys_info: SyscallInfo = pass_syscall_info();

    if status != TaskStatus::Running {
        return -1;
    }

    // translate virtual address to physical address.
    // hard cases can exist, for we stored too much in the syscall_tables.
    unsafe {
        let taskinfo_va = &((*_ti).status) as *const _ as usize;
        let syscall_va_base = &((*_ti).syscall_times[0]) as *const _ as usize;
        let time_va = &((*_ti).time) as *const _ as usize;
    
        let status_pa = translated_refmut(token, taskinfo_va as *mut TaskStatus);
        let time_pa = translated_refmut(token, time_va as *mut usize);
        
        *status_pa = TaskStatus::Running;
        // precision problem.
        *time_pa = (get_time_us() / 1000) - sys_info.time;
        for i in 0..MAX_SYSCALL_NUM {
            let syscall_pa = translated_refmut(token, (syscall_va_base + 4 * i) as *mut u32);
            *syscall_pa = sys_info.syscall_times[i];
        }
    }
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // Check whether _start, _port are valid.
    // first, check some parameters for mapArea.
    let start_va = VirtAddr::from(_start);
    // start_va should be of no offsets.
    if start_va.page_offset() != 0 {
        return -1;
    }
    let end_va = VirtAddr::from(_len + _start);
    if _port & 0x7 == 0 || _port & !0x7 != 0 {
        return -1;
    }
    let mut map_perm = MapPermission::U;
    if let 1 = _port & 0x1 {
        map_perm |= MapPermission::R;
    }
    if let 2 = _port & 0x2 {
        map_perm |= MapPermission::W;
    }
    if let 4 = _port & 0x4 {
        map_perm |= MapPermission::X;
    }
    // push the va into current task.
    push_current_area(start_va, end_va, map_perm)
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // Check whether _start is valid.
    let start_va = VirtAddr::from(_start);
    // start_va should be aligned for multi of 4 KB.
    if start_va.page_offset() != 0 {
        return -1;
    }
    let end_va = VirtAddr::from(_len + _start);

    release_current_area(start_va, end_va)
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    // println!("path:{:?}", _path);

    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED _path:{:?}",
        current_task().unwrap().pid.0, _path
    );
    
    // get the current_task
    let current_task = current_task().unwrap();
    // Two cases:
    // 1. if the _path is a valid thing, we can simply load it. Therefore we don't have to use fork, we can use vfork instead.
    // 2. if the _path is not valid, we don't allocate a new process.
    let token = current_task.get_user_token();
    let path = translated_str(token, _path);

    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let new_task: Arc<TaskControlBlock> = current_task.spawn(all_data.as_slice());
        let new_pid = new_task.pid.0 as isize;

        trace!(
            "task:{:?}", path.as_str()
        );
        // load the data.
        add_task(new_task);
        new_pid
    } else {
        -1
    }

}

/// YOUR JOB: Set task priority.
/// Set task priority
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _prio <= 1 {
        return -1;
    }
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    let u_prio = _prio as usize;
    inner.taskinfo.priority = u_prio;
    inner.taskinfo.pass = BIG_STRIDE / u_prio;
    drop(inner);
    _prio
}
