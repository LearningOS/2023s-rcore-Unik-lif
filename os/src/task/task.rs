//! Types related to task management & Functions for completely changing TCB
use super::TaskContext;
use super::{kstack_alloc, pid_alloc, KernelStack, PidHandle};
use crate::config::{TRAP_CONTEXT_BASE, MAX_SYSCALL_NUM, BIG_STRIDE};
use crate::fs::{File, Stdin, Stdout};
use crate::mm::{MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE, MapPermission, judge_allocation, judge_free};
use crate::sync::UPSafeCell;
use crate::trap::{trap_handler, TrapContext};
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefMut;
use core::cmp::Ordering;

/// Task control block structure
///
/// Directly save the contents that will not change during running
pub struct TaskControlBlock {
    // Immutable
    /// Process identifier
    pub pid: PidHandle,

    /// Kernel stack corresponding to PID
    pub kernel_stack: KernelStack,

    /// Mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    /// Get the mutable reference of the inner TCB
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    /// Get the address of app's page table
    pub fn get_user_token(&self) -> usize {
        let inner = self.inner_exclusive_access();
        inner.memory_set.token()
    }

    /// Lab2:
    /// push area to the current task control blocks.
    pub fn push_current_area(&self, start_va: VirtAddr, end_va: VirtAddr, permission: MapPermission) -> isize {
        let token = self.get_user_token();
        
        // judge whether the page allocated before or not.
        if let None = judge_allocation(token, start_va, end_va) {
            return -1;
        }

        let mut inner = self.inner_exclusive_access();
        // not allocated before, so we simply use insert_framed_area here to finish our mappings.
        inner.memory_set.insert_framed_area(start_va, end_va, permission);
        0
    }

    /// Lab2:
    /// release area from the current task control blocks.
    pub fn release_current_area(&self, start_va: VirtAddr, end_va: VirtAddr) -> isize {
        let token = self.get_user_token();
        
        // judge whether the page allocated before or not.
        if let None = judge_free(token, start_va, end_va) {
            return -1;
        }

        let mut inner = self.inner_exclusive_access();
        // not freed before, so we simply use set_munmap to release this part.
        if inner.memory_set.set_munmap(start_va, end_va) == false {
            return -1;
        }
        0
    }
    /// Lab3: 
    /// return the stride of the task.
    pub fn get_stride(&self) -> Stride {
        self.inner_exclusive_access().taskinfo.stride
    }
}



pub struct TaskControlBlockInner {
    /// The physical page number of the frame where the trap context is placed
    pub trap_cx_ppn: PhysPageNum,

    /// Application data can only appear in areas
    /// where the application address space is lower than base_size
    pub base_size: usize,

    /// Save task context
    pub task_cx: TaskContext,

    /// Maintain the execution status of the current process
    pub task_status: TaskStatus,

    /// Application address space
    pub memory_set: MemorySet,

    /// Parent process of the current process.
    /// Weak will not affect the reference count of the parent
    pub parent: Option<Weak<TaskControlBlock>>,

    /// A vector containing TCBs of all child processes of the current process
    pub children: Vec<Arc<TaskControlBlock>>,

    /// It is set when active exit or execution error occurs
    pub exit_code: i32,
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,

    /// Heap bottom
    pub heap_bottom: usize,

    /// Program break
    pub program_brk: usize,

    /// Lab1: The task info
    /// Syscall info
    pub taskinfo: SyscallInfo,

    // Lab4: The BMap tree
    // Get the stat of the fd.
    //pub map: BTreeMap<usize, Stat>,
    //pub namemap: BTreeMap<String, usize>,
}

impl TaskControlBlockInner {
    /// get the trap context
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }
    /// get the user token
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    pub fn get_status(&self) -> TaskStatus {
        self.task_status
    }
    pub fn is_zombie(&self) -> bool {
        self.get_status() == TaskStatus::Zombie
    }
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }

    pub fn get_taskinfo(&self) -> SyscallInfo {
        self.taskinfo
    }

    pub fn add_one_syscall(&mut self, sys_num: usize) {
        self.taskinfo.syscall_times[sys_num] += 1;
    }

}

impl TaskControlBlock {
    /// Create a new process
    ///
    /// At present, it is only used for the creation of initproc
    pub fn new(elf_data: &[u8]) -> Self {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        // push a task context which goes to trap_return to the top of kernel stack
        let task_control_block = Self {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    heap_bottom: user_sp,
                    program_brk: user_sp,
                    taskinfo: SyscallInfo {
                        syscall_times: [0; MAX_SYSCALL_NUM],
                        time: 0,
                        stride: Stride(0),
                        pass: BIG_STRIDE / 16,
                        priority: 16,
                    },
                    //map: BTreeMap::<usize, Stat>::new(),
                    //namemap: BTreeMap::<String, usize>::new(),
                })
            },
        };
        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        task_control_block
    }

    /// Load a new elf to replace the original application address space and start execution
    pub fn exec(&self, elf_data: &[u8]) {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();

        // **** access current TCB exclusively
        let mut inner = self.inner_exclusive_access();
        // substitute memory_set
        inner.memory_set = memory_set;
        // update trap_cx ppn
        inner.trap_cx_ppn = trap_cx_ppn;
        // initialize trap_cx
        let trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );
        *inner.get_trap_cx() = trap_cx;
        // **** release current PCB
    }

    /// parent process fork the child process
    pub fn fork(self: &Arc<TaskControlBlock>) -> Arc<TaskControlBlock> {
        // ---- hold parent PCB lock
        let mut parent_inner = self.inner_exclusive_access();
        // copy user space(include trap context)
        let memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent_inner.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        /*
        // copy map.
        let mut new_map: BTreeMap<usize, Stat> = BTreeMap::new();
        for (fd, stat) in parent_inner.map.iter() {
            new_map.insert(*fd, (*stat).clone());
        }

        // copy namemap.
        let mut new_namemap: BTreeMap<String, usize> = BTreeMap::new();
        for (name, fd) in parent_inner.namemap.iter() {
            new_namemap.insert(*name, *fd);
        }
        */
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    heap_bottom: parent_inner.heap_bottom,
                    program_brk: parent_inner.program_brk,
                    taskinfo: SyscallInfo {
                        syscall_times: [0; MAX_SYSCALL_NUM],
                        time: 0,
                        stride: Stride(0),
                        pass: BIG_STRIDE / 16,
                        priority: 16,
                    },
                    //map: new_map,
                    //namemap: new_namemap,
                })
            },
        });
        // add child
        parent_inner.children.push(task_control_block.clone());
        // modify kernel_sp in trap_cx
        // **** access child PCB exclusively
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kernel_stack_top;
        // return
        task_control_block
        // **** release child PCB
        // ---- release parent PCB
    }

    /// lab3: reproduce
    pub fn spawn(self: &Arc<TaskControlBlock>, elf_data: &[u8]) -> Arc<TaskControlBlock> {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        // push a task context which goes to trap_return to the top of kernel stack
        
        let mut father_inner = self.inner_exclusive_access();
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in father_inner.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        /*
        // copy namemap.
        let mut new_namemap: BTreeMap<String, usize> = BTreeMap::new();
        for (name, fd) in father_inner.namemap.iter() {
            new_namemap.insert(*name, *fd);
        }

        // copy map.
        let mut new_map: BTreeMap<usize, Stat> = BTreeMap::new();
        for (fd, stat) in father_inner.map.iter() {
            new_map.insert(*fd, (*stat).clone());
        }
        */
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    heap_bottom: user_sp,
                    program_brk: user_sp,
                    taskinfo: SyscallInfo {
                        syscall_times: [0; MAX_SYSCALL_NUM],
                        time: 0,
                        stride: Stride(0),
                        pass: BIG_STRIDE / 16,
                        priority: 16,
                    },
                    // map: new_map,
                    // namemap: new_namemap,
                })
            },
        });
        father_inner.children.push(task_control_block.clone());
        drop(father_inner);
        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        task_control_block
    }

    /// get pid of process
    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    /// change the location of the program break. return None if failed.
    pub fn change_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner_exclusive_access();
        let heap_bottom = inner.heap_bottom;
        let old_break = inner.program_brk;
        let new_brk = inner.program_brk as isize + size as isize;
        if new_brk < heap_bottom as isize {
            return None;
        }
        let result = if size < 0 {
            inner
                .memory_set
                .shrink_to(VirtAddr(heap_bottom), VirtAddr(new_brk as usize))
        } else {
            inner
                .memory_set
                .append_to(VirtAddr(heap_bottom), VirtAddr(new_brk as usize))
        };
        if result {
            inner.program_brk = new_brk as usize;
            Some(old_break)
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
/// task status: UnInit, Ready, Running, Exited
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Zombie,
}

/// Lab3:
/// implement Stride type here. 
#[derive(Copy, Clone)]
pub struct Stride(usize);

impl Stride {
    // initialize:
    pub fn new(_t: usize) -> Self {
        Stride(_t)
    }
}

impl PartialOrd for Stride {
    // We tend to return the min value.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.0 - other.0 > BIG_STRIDE / 2 {
            return Some(self.0.cmp(&other.0));
        }
        Some(other.0.cmp(&self.0))
    }
}


impl PartialEq for Stride {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}


/// Lab1: my taskinfo.
/// The syscall info of a task.
#[derive(Copy, Clone)]
pub struct SyscallInfo {
    /// The numbers of syscall called by task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of a task.
    pub time: usize,
    /// Stride so far.
    pub stride: Stride,
    /// Every Pass for a stride.
    pub pass: usize,
    /// Priority of the task.
    pub priority: usize,
}

impl SyscallInfo {
    /// add pass for the stride.
    pub fn add_stride(&mut self) {
        self.stride.0 += self.pass;
    }
}
