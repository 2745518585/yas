use super::ScanOptions;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};

pub type SharedJob = Arc<Mutex<Job>>;

#[derive(Clone, Serialize)]
pub struct LogLine {
    pub seq: u64,
    pub text: String,
}

pub struct Job {
    pub id: String,
    pub owner: String,
    pub state: &'static str,
    pub error: Option<String>,
    pub result: Option<Value>,
    pub logs: VecDeque<LogLine>,
    pub sequence: u64,
    pub touched: Instant,
    child: Child,
    _process_group: ProcessGroup,
}

// Windows closes this handle even if the console is closed or the bridge crashes.
// KILL_ON_JOB_CLOSE ensures its scanning child cannot keep controlling the mouse.
struct ProcessGroup(isize);
impl ProcessGroup {
    fn attach(child: &mut Child) -> Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::last_os_error().into());
            }
            let group = Self(handle as isize);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
            {
                let error = std::io::Error::last_os_error();
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
            Ok(group)
        }
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0 as _);
        }
    }
}

impl Job {
    pub fn running(&self) -> bool {
        matches!(self.state, "running" | "cancelling")
    }
    pub fn cancel(&mut self) -> Result<()> {
        if self.running() {
            if self.child.try_wait()?.is_none() {
                self.child.kill().context("无法结束扫描进程")?;
            }
            self.state = "cancelling";
        }
        Ok(())
    }
    fn log(&mut self, text: String) {
        self.sequence += 1;
        self.logs.push_back(LogLine {
            seq: self.sequence,
            text,
        });
        if self.logs.len() > 2000 {
            self.logs.pop_front();
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if self.running() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn read_logs(reader: impl Read + Send + 'static, job: SharedJob) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => job.lock().unwrap().log(line),
                Err(error) => {
                    job.lock().unwrap().log(format!("日志读取失败：{error}"));
                    break;
                },
            }
        }
    })
}

fn read_result(directory: &std::path::Path) -> Result<Value> {
    let data = std::fs::read(directory.join("mona.json")).context("扫描未生成 mona.json")?;
    let result: Value = serde_json::from_slice(&data).context("无法读取扫描结果")?;
    let mut count = 0;
    for slot in ["flower", "feather", "sand", "cup", "head"] {
        count += result
            .get(slot)
            .and_then(Value::as_array)
            .context("扫描结果格式无效")?
            .len();
    }
    if count == 0 {
        bail!("未识别到圣遗物，请检查背包页面与扫描条件");
    }
    Ok(result)
}

pub fn start(options: ScanOptions, owner: String, prefix: &[&str]) -> Result<SharedJob> {
    options.validate()?;
    if !super::platform::windows()
        .iter()
        .any(|window| window.hwnd == options.hwnd)
    {
        bail!("所选游戏窗口已关闭，请刷新窗口列表");
    }
    let directory = tempfile::Builder::new().prefix("yas-web-").tempdir()?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(prefix)
        .args([
            "--no-pause",
            "--hwnd",
            &options.hwnd.to_string(),
            "--min-star",
            &options.min_star.to_string(),
            "--min-level",
            &options.min_level.to_string(),
            "--format",
            "mona",
            "--output-dir",
        ])
        .arg(directory.path());
    if options.number > 0 {
        command.args(["--number", &options.number.to_string()]);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000 /* CREATE_NO_WINDOW */)
        .spawn()
        .context("无法启动扫描进程")?;
    let process_group = ProcessGroup::attach(&mut child).context("无法管理扫描进程")?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let job = Arc::new(Mutex::new(Job {
        id: format!("{:032x}", rand::random::<u128>()),
        owner,
        state: "running",
        error: None,
        result: None,
        logs: VecDeque::new(),
        sequence: 0,
        touched: Instant::now(),
        child,
        _process_group: process_group,
    }));
    let out = read_logs(stdout, job.clone());
    let err = read_logs(stderr, job.clone());
    let worker_job = job.clone();
    thread::spawn(move || {
        let started = Instant::now();
        let exit = loop {
            {
                let mut job = worker_job.lock().unwrap();
                // Stop unattended scans when the browser closes or the scan hangs.
                if job.state == "running"
                    && (job.touched.elapsed() > Duration::from_secs(120)
                        || started.elapsed() > Duration::from_secs(1800))
                {
                    job.log("网页连接中断或扫描超时，正在取消扫描。".into());
                    let _ = job.cancel();
                }
                match job.child.try_wait() {
                    Ok(Some(status)) => break Ok(status),
                    Ok(None) => {},
                    Err(error) => {
                        let _ = job.child.kill();
                        let _ = job.child.wait();
                        break Err(error);
                    },
                }
            }
            thread::sleep(Duration::from_millis(100));
        };
        let _ = out.join();
        let _ = err.join();
        let mut job = worker_job.lock().unwrap();
        if job.state == "cancelling" {
            job.state = "cancelled";
        } else {
            let result = match exit {
                Ok(status) if status.success() => read_result(directory.path()),
                Ok(status) => Err(anyhow::anyhow!("扫描进程退出：{status}，请查看日志")),
                Err(error) => Err(error.into()),
            };
            match result {
                Ok(result) => {
                    job.result = Some(result);
                    job.state = "completed";
                },
                Err(error) => {
                    job.error = Some(format!("{error:#}"));
                    job.state = "failed";
                },
            }
        }
        // TempDir is removed only after the child exits and its output is read.
    });
    Ok(job)
}
