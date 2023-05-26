//! File and filesystem-related syscalls
use crate::fs::{open_file, OpenFlags, Stat, ROOT_INODE, search_file, StatMode, add_link, unlink};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer, translated_refmut};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );

    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    
    if let Some(file) = &inner.fd_table[_fd] {
        let file = file.clone();
        let (id, mode) = file.get_stat();
        unsafe {
            let dev_va = &((*_st).dev) as *const _ as usize;
            let ino_va = &((*_st).ino) as *const _ as usize;
            let mode_va = &((*_st).mode) as *const _ as usize;
            let nlink_va = &((*_st).nlink) as *const _ as usize;

            let dev_pa = translated_refmut(token, dev_va as *mut u64);
            let ino_pa = translated_refmut(token, ino_va as *mut u64);
            let mode_pa = translated_refmut(token, mode_va as *mut StatMode);
            let nlink_pa = translated_refmut(token, nlink_va as *mut u32);
            
            *dev_pa = 0;
            *ino_pa = id;
            *mode_pa = mode;
            *nlink_pa = ROOT_INODE.find_by_ino(id);
        }
        0
    } else {
        -1
    }

}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    // let task = current_task().unwrap();
    // let mut inner = task.inner_exclusive_access();
    let old_path = translated_str(token, _old_name);
    let new_path = translated_str(token, _new_name);

    // first judge whether _old_name is equal to _new_name.
    if old_path == new_path {
        return -1;
    }
    // second, judge whether the new_path exists?
    if let Some(_) = search_file(new_path.as_str()) {
        return -1;
    }
    // third, judge whether the old_path exists?
    if let None = search_file(old_path.as_str()) {
        return -1;
    }
    
    if add_link(old_path.as_str(), new_path.as_str()) == -1 {
        return -1;
    }
    0
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, _name);
    if unlink(path.as_str()) == -1 {
        return -1;
    }
    0
}
