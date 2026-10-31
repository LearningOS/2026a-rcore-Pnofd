use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{
    block_current_and_run_next, current_process, current_task, ProcessControlBlock,
    TaskStatus,
};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec;

/// Value returned when a request would create a deadlock.
const DEADLOCK: isize = -0xDEAD;

/// Thread id of the currently running task.
fn current_tid() -> usize {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid
}

/// Banker's algorithm safety check for a semaphore request.
///
/// `Available` comes from the current semaphore counts, `Allocation` from the
/// units each thread already holds and `Need` from the pending `down` calls.
fn sem_request_is_safe(
    process: &Arc<ProcessControlBlock>,
    req_tid: usize,
    req_sem: usize,
) -> bool {
    let process_inner = process.inner_exclusive_access();
    let sem_num = process_inner.semaphore_list.len();
    if sem_num == 0 {
        return true;
    }
    let task_num = process_inner.tasks.len();
    let mut available = vec![0usize; sem_num];
    for (j, sem) in process_inner.semaphore_list.iter().enumerate() {
        if let Some(sem) = sem {
            let count = sem.inner.exclusive_access().count;
            available[j] = if count > 0 { count as usize } else { 0 };
        }
    }
    let mut allocation = vec![vec![0usize; sem_num]; task_num];
    let mut need = vec![vec![0usize; sem_num]; task_num];
    for (tid, task) in process_inner.tasks.iter().enumerate() {
        if let Some(task) = task {
            let task_inner = task.inner_exclusive_access();
            for j in 0..sem_num {
                allocation[tid][j] = *task_inner.sem_alloc.get(j).unwrap_or(&0);
            }
            if task_inner.task_status == TaskStatus::Blocked {
                if let Some(j) = task_inner.blocked_sem {
                    if j < sem_num {
                        need[tid][j] = 1;
                    }
                }
            }
        }
    }
    if req_tid < task_num && req_sem < sem_num {
        need[req_tid][req_sem] += 1;
    }

    let mut work = available;
    let mut finish = vec![false; task_num];
    loop {
        let mut progressed = false;
        for tid in 0..task_num {
            if finish[tid] {
                continue;
            }
            if (0..sem_num).all(|j| need[tid][j] <= work[j]) {
                for j in 0..sem_num {
                    work[j] = work[j].saturating_add(allocation[tid][j]);
                }
                finish[tid] = true;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    finish.iter().all(|finished| *finished)
}
/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() - 1
    };
    if process_inner.mutex_owner.len() <= id {
        process_inner.mutex_owner.resize(id + 1, None);
    }
    process_inner.mutex_owner[id] = None;
    id as isize
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let tid = current_tid();
    let (mutex, detect) = {
        let process_inner = process.inner_exclusive_access();
        if process_inner.deadlock_detect
            && process_inner
                .mutex_owner
                .get(mutex_id)
                .map_or(false, |owner| *owner == Some(tid))
        {
            return DEADLOCK;
        }
        (
            Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap()),
            process_inner.deadlock_detect,
        )
    };
    mutex.lock();
    if detect {
        let mut process_inner = process.inner_exclusive_access();
        if process_inner.mutex_owner.len() <= mutex_id {
            process_inner.mutex_owner.resize(mutex_id + 1, None);
        }
        process_inner.mutex_owner[mutex_id] = Some(tid);
    }
    0
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex = {
        let process_inner = process.inner_exclusive_access();
        Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap())
    };
    mutex.unlock();
    let mut process_inner = process.inner_exclusive_access();
    if process_inner.mutex_owner.len() > mutex_id {
        process_inner.mutex_owner[mutex_id] = None;
    }
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let sem = {
        let process_inner = process.inner_exclusive_access();
        Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap())
    };
    let task = current_task().unwrap();
    {
        let mut task_inner = task.inner_exclusive_access();
        if sem_id < task_inner.sem_alloc.len() && task_inner.sem_alloc[sem_id] > 0 {
            task_inner.sem_alloc[sem_id] -= 1;
        }
    }
    sem.up();
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let tid = current_tid();
    let (sem, detect) = {
        let process_inner = process.inner_exclusive_access();
        (
            Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap()),
            process_inner.deadlock_detect,
        )
    };
    if detect && !sem_request_is_safe(&process, tid, sem_id) {
        return DEADLOCK;
    }
    let task = current_task().unwrap();
    task.inner_exclusive_access().blocked_sem = Some(sem_id);
    sem.down();
    let mut task_inner = task.inner_exclusive_access();
    task_inner.blocked_sem = None;
    if task_inner.sem_alloc.len() <= sem_id {
        task_inner.sem_alloc.resize(sem_id + 1, 0);
    }
    task_inner.sem_alloc[sem_id] += 1;
    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
pub fn sys_enable_deadlock_detect(enabled: usize) -> isize {
    trace!("kernel: sys_enable_deadlock_detect");
    if enabled > 1 {
        return -1;
    }
    let process = current_process();
    process.inner_exclusive_access().deadlock_detect = enabled == 1;
    0
}
