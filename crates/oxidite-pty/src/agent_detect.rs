//! Foreground Agentic CLI detection combining:
//! 1. Cached single-snapshot descendant tree + bounded PEB/`/proc/pid/cmdline`
//!    `argv[0]/argv[1]` inspection (verified against shell start time to prevent PID reuse,
//!    with `zeroize` scrubbing of raw buffers/decoded strings and graceful fallback to `szExeFile`).
//! 2. OSC 0 / OSC 2 window title matching (only when an opaque child like `ssh`/`wsl` is active
//!    locally, or for remote sessions where `shell_pid` is unavailable, so exited local agents
//!    do not leave stale badges behind).
//!
//! Prioritizes known agent CLIs over ephemeral leaf utilities (`rg.exe`, `git.exe`,
//! `conhost.exe`, `OpenConsole.exe`) so tab badges never flicker mid-tool-call.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use zeroize::Zeroize;

#[cfg(unix)]
const MAX_CMDLINE_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentKind {
    Codex,
    ClaudeCode,
    Devin,
    Cursor,
    Antigravity,
}

impl AgentKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude",
            Self::Devin => "devin",
            Self::Cursor => "cursor",
            Self::Antigravity => "antigravity",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
            Self::Devin => "Devin",
            Self::Cursor => "Cursor",
            Self::Antigravity => "Antigravity",
        }
    }
}

/// Ignored helper/wrapper executables that should never override an active agent match.
const IGNORED_UTILITIES: &[&str] = &[
    "conhost.exe",
    "openconsole.exe",
    "rg.exe",
    "rg",
    "git.exe",
    "git",
    "git-remote-https.exe",
    "ssh.exe",
    "find.exe",
    "where.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "bash.exe",
    "sh.exe",
];

/// Matches an agent CLI from executable name (`argv0`), optional script path (`argv1`),
/// and optional OSC 0/2 terminal title.
pub fn detect_agent_from_argv_and_title(
    argv0: Option<&str>,
    argv1: Option<&str>,
    osc_title: Option<&str>,
) -> Option<AgentKind> {
    if let Some(a0) = argv0 {
        let base = basename_lower(a0);
        if !IGNORED_UTILITIES.contains(&base.as_str())
            && let Some(agent) = match_token(&base)
        {
            return Some(agent);
        }
        // Node / Bun / Python wrappers often run the actual agent CLI in argv[1]
        if let Some(a1) = argv1
            && let Some(agent) = match_script_path(a1)
        {
            return Some(agent);
        }
    } else if let Some(a1) = argv1
        && let Some(agent) = match_script_path(a1)
    {
        return Some(agent);
    }

    if let Some(title) = osc_title
        && let Some(agent) = match_title(title)
    {
        return Some(agent);
    }

    None
}

/// Script runtimes whose `argv[1]`-ish token names the program actually being run. Linux `comm`
/// is truncated to 15 bytes and carries version suffixes (`python3.12`), so match by family.
fn is_runtime_wrapper(base_lower: &str) -> bool {
    let stem = base_lower.strip_suffix(".exe").unwrap_or(base_lower);
    if matches!(stem, "node" | "nodejs" | "bun" | "deno") {
        return true;
    }
    stem.strip_prefix("python")
        .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

fn basename_lower(path: &str) -> String {
    let trimmed = path.trim().trim_matches('"');
    let name = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed);
    name.to_ascii_lowercase()
}

fn match_token(base_lower: &str) -> Option<AgentKind> {
    let stem = base_lower
        .strip_suffix(".exe")
        .or_else(|| base_lower.strip_suffix(".cmd"))
        .or_else(|| base_lower.strip_suffix(".ps1"))
        .unwrap_or(base_lower);

    match stem {
        "codex" => Some(AgentKind::Codex),
        "claude" | "claude-code" => Some(AgentKind::ClaudeCode),
        "devin" => Some(AgentKind::Devin),
        // Plain `cursor` is the IDE launcher, not the agent CLI: only `cursor-agent` counts.
        "cursor-agent" => Some(AgentKind::Cursor),
        "antigravity" | "agy" => Some(AgentKind::Antigravity),
        _ => None,
    }
}

fn match_script_path(argv1: &str) -> Option<AgentKind> {
    let lower = argv1.trim().trim_matches('"').to_ascii_lowercase();
    let norm = lower.replace('\\', "/");

    if norm.contains("@openai/codex")
        || norm.ends_with("/codex")
        || norm.ends_with("/codex.js")
        || norm.ends_with("/codex.mjs")
    {
        return Some(AgentKind::Codex);
    }
    if norm.contains("@anthropic-ai/claude-code")
        || norm.contains("/claude-code/")
        || norm.ends_with("/claude.js")
        || norm.ends_with("/claude.mjs")
        || norm.ends_with("/claude")
    {
        return Some(AgentKind::ClaudeCode);
    }
    if norm.contains("/devin/") || norm.ends_with("/devin") || norm.ends_with("/devin.js") {
        return Some(AgentKind::Devin);
    }
    if norm.contains("cursor-agent") {
        return Some(AgentKind::Cursor);
    }
    if norm.contains("/antigravity/") || norm.ends_with("/antigravity") || norm.ends_with("/agy") {
        return Some(AgentKind::Antigravity);
    }
    None
}

/// Child wrappers/tunnels whose internal processes are opaque to the host OS tree
/// (`ssh`, `wsl`, containers), making OSC window-title matching legitimate while active.
const OPAQUE_CONTAINERS: &[&str] = &[
    "ssh.exe",
    "ssh",
    "wsl.exe",
    "wsl",
    "wslhost.exe",
    "ubuntu.exe",
    "debian.exe",
    "docker.exe",
    "docker",
    "podman.exe",
    "podman",
    "tmux",
];

fn match_title(title: &str) -> Option<AgentKind> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();

    if lower.contains("claude code")
        || lower == "claude"
        || lower.starts_with("claude ")
        || lower.contains("✳ claude")
    {
        return Some(AgentKind::ClaudeCode);
    }
    if title_matches_cli_label(&lower, "codex", &["openai codex", "codex cli"]) {
        return Some(AgentKind::Codex);
    }
    if title_matches_cli_label(&lower, "antigravity", &["antigravity cli"]) {
        return Some(AgentKind::Antigravity);
    }
    if title_matches_cli_label(&lower, "devin", &["devin cli"]) {
        return Some(AgentKind::Devin);
    }
    if lower.contains("cursor agent") || lower == "cursor" {
        return Some(AgentKind::Cursor);
    }
    None
}

/// Matches explicit CLI phrases or standalone/prefixed title tokens while ignoring directory paths
/// such as `C:\Users\devin\code` or `/home/codex-app`.
fn title_matches_cli_label(lower: &str, keyword: &str, phrases: &[&str]) -> bool {
    if phrases.iter().any(|p| lower.contains(p)) {
        return true;
    }
    // Check only the leading title segment before common " — " / " - " / " | " separators
    // if the title itself is a filesystem path.
    let looks_like_path =
        lower.contains(":\\") || lower.starts_with('/') || lower.starts_with("~/");
    if looks_like_path {
        return false;
    }
    has_standalone_word(lower, keyword)
}

fn has_standalone_word(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(rel) = haystack[start..].find(needle) {
        let idx = start + rel;
        let before_ok = haystack[..idx]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && !"_-/\\@.".contains(c));
        let after_idx = idx + needle.len();
        let after_ok = haystack[after_idx..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && !"_-/\\@.".contains(c));
        if before_ok && after_ok {
            return true;
        }
        start = after_idx;
    }
    false
}

/// Max command-line tokens ever examined: `argv[0]`, runtime flags, then the script path.
const MAX_ARG_TOKENS: usize = 6;

/// Picks `argv[0]` and the first non-flag token (the script, so `node --inspect cli.js` works) out of
/// at most [`MAX_ARG_TOKENS`] leading tokens. Nothing after that window is ever inspected or retained,
/// so API keys or secret flags later on the command line stay unread.
pub fn extract_argv0_and_script<'a>(
    tokens: impl Iterator<Item = &'a str>,
) -> (Option<String>, Option<String>) {
    let mut a0 = None;
    let mut script = None;
    for (idx, tok) in tokens.take(MAX_ARG_TOKENS).enumerate() {
        if idx == 0 {
            a0 = Some(tok.to_string());
        } else if !tok.starts_with('-') {
            script = Some(tok.to_string());
            break;
        }
    }
    (a0, script)
}

/// Splits a Windows-style command line (double quotes group tokens) and runs
/// [`extract_argv0_and_script`] over its leading tokens.
pub fn extract_first_two_args(cmdline: &str) -> (Option<String>, Option<String>) {
    let mut tokens: Vec<String> = Vec::with_capacity(MAX_ARG_TOKENS);
    let mut current = String::new();
    let mut in_quotes = false;
    let flush = |cur: &mut String, toks: &mut Vec<String>| {
        if !cur.is_empty() {
            toks.push(std::mem::take(cur));
        }
    };

    for ch in cmdline.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                flush(&mut current, &mut tokens);
                if tokens.len() >= MAX_ARG_TOKENS {
                    break;
                }
            }
            c => current.push(c),
        }
    }
    flush(&mut current, &mut tokens);
    let out = extract_argv0_and_script(tokens.iter().map(String::as_str));
    for t in &mut tokens {
        t.zeroize();
    }
    current.zeroize();
    out
}

#[derive(Debug, Clone)]
struct ProcNode {
    pid: u32,
    ppid: u32,
    exe_name: String,
    #[cfg_attr(not(unix), allow(dead_code))]
    start_time: Option<u64>,
}

struct CachedSnapshot {
    taken_at: Instant,
    children_by_ppid: HashMap<u32, Vec<ProcNode>>,
}

/// Process-tree snapshot cache shared across all sessions so a single `list_sessions`
/// poll takes at most ONE OS snapshot every `ttl` (default 2s).
pub struct ProcessProbeCache {
    ttl: Duration,
    cache: Mutex<Option<CachedSnapshot>>,
}

impl Default for ProcessProbeCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}

impl ProcessProbeCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            cache: Mutex::new(None),
        }
    }

    /// Detects the active agentic CLI running under `shell_pid` (if any), falling back
    /// to the session's captured OSC 0/2 title.
    ///
    /// Invoke inside `tokio::task::spawn_blocking` so snapshot/PEB syscalls never touch
    /// async worker threads.
    pub fn detect_for_shell(&self, shell_pid: u32, osc_title: Option<&str>) -> Option<AgentKind> {
        let descendants = self.descendants_of(shell_pid);
        let mut has_opaque_child = false;

        // Walk the entire descendant subtree and prioritize ANY known agent match over
        // ephemeral leaf utilities like `rg.exe` or `git.exe`.
        for node in descendants {
            let base = node.exe_name.to_ascii_lowercase();
            // Linux `comm` for tmux is "tmux: server" / "tmux: client".
            let opaque_name = base.split(':').next().unwrap_or(&base).trim();
            if OPAQUE_CONTAINERS.contains(&opaque_name) {
                has_opaque_child = true;
            }
            if IGNORED_UTILITIES.contains(&base.as_str()) {
                continue;
            }
            // Fast path: exe name directly identifies the agent (no PEB read needed).
            if let Some(agent) = detect_agent_from_argv_and_title(Some(&node.exe_name), None, None)
            {
                return Some(agent);
            }
            // Runtime wrapper path (`node.exe`, `bun.exe`, `python.exe`): inspect argv[0..2] via PEB.
            if is_runtime_wrapper(&base)
                && let Some((mut a0, mut a1)) = read_process_argv01(node.pid)
            {
                let found = detect_agent_from_argv_and_title(
                    a0.as_deref().or(Some(&node.exe_name)),
                    a1.as_deref(),
                    None,
                );
                // The script slot can hold a flag value (e.g. `--api-key sk-...`); never keep it around.
                a0.zeroize();
                a1.zeroize();
                if found.is_some() {
                    return found;
                }
            }
        }

        // Only consult OSC title when an opaque child (`ssh`, `wsl`, `docker`, `tmux`) is active,
        // so exited local CLIs never leave stale title badges behind.
        if has_opaque_child {
            detect_agent_from_argv_and_title(None, None, osc_title)
        } else {
            None
        }
    }

    fn descendants_of(&self, root_pid: u32) -> Vec<ProcNode> {
        let mut guard = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let needs_refresh = guard
            .as_ref()
            .is_none_or(|snap| snap.taken_at.elapsed() >= self.ttl);

        if needs_refresh {
            let procs = take_process_snapshot();
            let mut by_ppid: HashMap<u32, Vec<ProcNode>> = HashMap::new();
            for p in procs {
                by_ppid.entry(p.ppid).or_default().push(p);
            }
            *guard = Some(CachedSnapshot {
                taken_at: Instant::now(),
                children_by_ppid: by_ppid,
            });
        }

        let snap = guard.as_ref().unwrap();
        let root_start = find_process_start_time(snap, root_pid);
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back(root_pid);
        visited.insert(root_pid);

        while let Some(pid) = queue.pop_front() {
            if let Some(children) = snap.children_by_ppid.get(&pid) {
                for child in children {
                    if !is_not_older_than_root(root_start, child) {
                        continue;
                    }
                    if visited.insert(child.pid) {
                        result.push(child.clone());
                        queue.push_back(child.pid);
                    }
                }
            }
        }
        result
    }
}

fn find_process_start_time(snap: &CachedSnapshot, target_pid: u32) -> Option<u64> {
    for nodes in snap.children_by_ppid.values() {
        if let Some(node) = nodes.iter().find(|n| n.pid == target_pid)
            && let Some(ts) = node.start_time
        {
            return Some(ts);
        }
    }
    query_process_start_time(target_pid)
}

fn is_not_older_than_root(root_start: Option<u64>, child: &ProcNode) -> bool {
    let Some(root_ts) = root_start else {
        return true;
    };
    let child_ts = child
        .start_time
        .or_else(|| query_process_start_time(child.pid));
    child_ts.is_none_or(|cts| cts >= root_ts)
}

#[cfg(windows)]
fn query_process_start_time(pid: u32) -> Option<u64> {
    win_probe::process_creation_time(pid)
}

#[cfg(not(windows))]
fn query_process_start_time(_pid: u32) -> Option<u64> {
    None
}

#[cfg(windows)]
fn take_process_snapshot() -> Vec<ProcNode> {
    win_probe::snapshot_processes()
}

#[cfg(windows)]
fn read_process_argv01(pid: u32) -> Option<(Option<String>, Option<String>)> {
    win_probe::read_peb_argv01(pid)
}

#[cfg(unix)]
fn take_process_snapshot() -> Vec<ProcNode> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return out;
    };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let stat_path = entry.path().join("stat");
        let Ok(stat) = std::fs::read_to_string(stat_path) else {
            continue;
        };
        if let (Some(l), Some(r)) = (stat.find('('), stat.rfind(')')) {
            // A truncated read can end at the closing paren; skip the entry instead of panicking.
            let (Some(exe), Some(tail)) = (stat.get(l + 1..r), stat.get(r + 1..)) else {
                continue;
            };
            let exe_name = exe.to_string();
            let rest: Vec<&str> = tail.split_whitespace().collect();
            if let Some(ppid) = rest.get(1).and_then(|s| s.parse::<u32>().ok()) {
                // `/proc/<pid>/stat` field 22 (index 19 after comm/state) is process starttime.
                let start_time = rest.get(19).and_then(|s| s.parse::<u64>().ok());
                out.push(ProcNode {
                    pid,
                    ppid,
                    exe_name,
                    start_time,
                });
            }
        }
    }
    out
}

#[cfg(unix)]
fn read_process_argv01(pid: u32) -> Option<(Option<String>, Option<String>)> {
    use std::io::Read;
    let file = std::fs::File::open(format!("/proc/{pid}/cmdline")).ok()?;
    let mut bytes = Vec::with_capacity(256);
    file.take(MAX_CMDLINE_BYTES as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    let tokens: Vec<String> = bytes
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .take(MAX_ARG_TOKENS)
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    bytes.zeroize();
    let mut tokens = tokens;
    let pair = extract_argv0_and_script(tokens.iter().map(String::as_str));
    for t in &mut tokens {
        t.zeroize();
    }
    Some(pair)
}

#[cfg(windows)]
mod win_probe {
    use super::{ProcNode, extract_first_two_args};
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;
    use zeroize::Zeroize;

    type Handle = *mut c_void;
    type Bool = i32;
    type NtStatus = i32;

    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const PROCESS_VM_READ: u32 = 0x0010;
    const PROCESS_BASIC_INFORMATION_CLASS: u32 = 0;
    const MAX_CMDLINE_BYTES: usize = 8192;

    #[repr(C)]
    struct ProcessEntry32W {
        dw_size: u32,
        cnt_usage: u32,
        th32_process_id: u32,
        th32_default_heap_id: usize,
        th32_module_id: u32,
        cnt_threads: u32,
        th32_parent_process_id: u32,
        pc_pri_class_base: i32,
        dw_flags: u32,
        sz_exe_file: [u16; 260],
    }

    #[repr(C)]
    struct ProcessBasicInformation {
        reserved1: *mut c_void,
        peb_base_address: *mut c_void,
        reserved2: [*mut c_void; 2],
        unique_process_id: usize,
        reserved3: *mut c_void,
    }

    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum_length: u16,
        buffer: *const u16,
    }

    #[repr(C)]
    struct FileTime {
        dw_low_date_time: u32,
        dw_high_date_time: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> Handle;
        fn Process32FirstW(h_snapshot: Handle, lppe: *mut ProcessEntry32W) -> Bool;
        fn Process32NextW(h_snapshot: Handle, lppe: *mut ProcessEntry32W) -> Bool;
        fn OpenProcess(
            dw_desired_access: u32,
            b_inherit_handle: Bool,
            dw_process_id: u32,
        ) -> Handle;
        fn GetProcessTimes(
            h_process: Handle,
            lp_creation_time: *mut FileTime,
            lp_exit_time: *mut FileTime,
            lp_kernel_time: *mut FileTime,
            lp_user_time: *mut FileTime,
        ) -> Bool;
        fn IsWow64Process(h_process: Handle, wow64_process: *mut Bool) -> Bool;
        fn ReadProcessMemory(
            h_process: Handle,
            lp_base_address: *const c_void,
            lp_buffer: *mut c_void,
            n_size: usize,
            lp_number_of_bytes_read: *mut usize,
        ) -> Bool;
        fn CloseHandle(h_object: Handle) -> Bool;
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationProcess(
            process_handle: Handle,
            process_information_class: u32,
            process_information: *mut c_void,
            process_information_length: u32,
            return_length: *mut u32,
        ) -> NtStatus;
    }

    struct HandleGuard(Handle);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    pub fn snapshot_processes() -> Vec<ProcNode> {
        let mut nodes = Vec::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap.is_null() || snap == INVALID_HANDLE_VALUE {
                return nodes;
            }
            let _guard = HandleGuard(snap);

            let mut entry: ProcessEntry32W = zeroed();
            entry.dw_size = u32::try_from(size_of::<ProcessEntry32W>()).unwrap_or(u32::MAX);

            if Process32FirstW(snap, &raw mut entry) != 0 {
                loop {
                    let end = entry
                        .sz_exe_file
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.sz_exe_file.len());
                    let exe_name = String::from_utf16_lossy(&entry.sz_exe_file[..end]);
                    nodes.push(ProcNode {
                        pid: entry.th32_process_id,
                        ppid: entry.th32_parent_process_id,
                        exe_name,
                        start_time: None,
                    });
                    if Process32NextW(snap, &raw mut entry) == 0 {
                        break;
                    }
                }
            }
        }
        nodes
    }

    pub fn process_creation_time(pid: u32) -> Option<u64> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let _guard = HandleGuard(h);
            let mut creation: FileTime = zeroed();
            let mut exit: FileTime = zeroed();
            let mut kernel: FileTime = zeroed();
            let mut user: FileTime = zeroed();
            if GetProcessTimes(
                h,
                &raw mut creation,
                &raw mut exit,
                &raw mut kernel,
                &raw mut user,
            ) == 0
            {
                return None;
            }
            Some(
                (u64::from(creation.dw_high_date_time) << 32)
                    | u64::from(creation.dw_low_date_time),
            )
        }
    }

    /// Reads ONLY `argv[0]` and `argv[1]` from a 64-bit child process PEB.
    /// Returns `None` gracefully on `ACCESS_DENIED`, `WoW64` 32-bit child, or race exits.
    #[cfg(target_pointer_width = "64")]
    pub fn read_peb_argv01(pid: u32) -> Option<(Option<String>, Option<String>)> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
            if h.is_null() {
                return None;
            }
            let _guard = HandleGuard(h);

            // Skip WoW64 32-bit children cleanly (fall back to szExeFile).
            let mut is_wow64: Bool = 0;
            if IsWow64Process(h, &raw mut is_wow64) == 0 || is_wow64 != 0 {
                return None;
            }

            let mut pbi: ProcessBasicInformation = zeroed();
            let mut ret_len: u32 = 0;
            let status = NtQueryInformationProcess(
                h,
                PROCESS_BASIC_INFORMATION_CLASS,
                (&raw mut pbi).cast::<c_void>(),
                u32::try_from(size_of::<ProcessBasicInformation>()).unwrap_or(u32::MAX),
                &raw mut ret_len,
            );
            if status < 0 || pbi.peb_base_address.is_null() {
                return None;
            }

            // On x64 Windows: PEB + 0x20 == ProcessParameters (PRTL_USER_PROCESS_PARAMETERS)
            let proc_params_ptr_addr = (pbi.peb_base_address as usize).checked_add(0x20)?;
            let mut proc_params: *const c_void = null_mut();
            if !read_struct(h, proc_params_ptr_addr as *const c_void, &mut proc_params)
                || proc_params.is_null()
            {
                return None;
            }

            // On x64 Windows: RTL_USER_PROCESS_PARAMETERS + 0x70 == CommandLine (UNICODE_STRING)
            let cmdline_ustr_addr = (proc_params as usize).checked_add(0x70)?;
            let mut cmdline_ustr: UnicodeString = zeroed();
            if !read_struct(h, cmdline_ustr_addr as *const c_void, &mut cmdline_ustr)
                || cmdline_ustr.buffer.is_null()
                || cmdline_ustr.length == 0
            {
                return None;
            }

            let byte_len = (cmdline_ustr.length as usize).min(MAX_CMDLINE_BYTES);
            let u16_len = byte_len / 2;
            let mut buf = vec![0u16; u16_len];
            let mut bytes_read: usize = 0;
            let ok = ReadProcessMemory(
                h,
                cmdline_ustr.buffer.cast::<c_void>(),
                buf.as_mut_ptr().cast::<c_void>(),
                u16_len * 2,
                &raw mut bytes_read,
            );
            if ok == 0 || bytes_read < 2 {
                buf.zeroize();
                return None;
            }

            let valid_u16s = (bytes_read / 2).min(buf.len());
            let mut decoded = String::from_utf16_lossy(&buf[..valid_u16s]);
            let pair = extract_first_two_args(&decoded);

            // Scrub raw and decoded command-line buffers with volatile zeroize so secrets cannot linger.
            buf.zeroize();
            decoded.zeroize();

            Some(pair)
        }
    }

    #[cfg(not(target_pointer_width = "64"))]
    pub fn read_peb_argv01(_pid: u32) -> Option<(Option<String>, Option<String>)> {
        None
    }

    #[cfg(target_pointer_width = "64")]
    unsafe fn read_struct<T>(h: Handle, addr: *const c_void, out: &mut T) -> bool {
        let mut read = 0usize;
        unsafe {
            ReadProcessMemory(
                h,
                addr,
                std::ptr::from_mut::<T>(out).cast::<c_void>(),
                size_of::<T>(),
                &raw mut read,
            ) != 0
                && read == size_of::<T>()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_direct_binaries_and_ignores_utilities() {
        assert_eq!(
            detect_agent_from_argv_and_title(Some("codex.exe"), None, None),
            Some(AgentKind::Codex)
        );
        assert_eq!(
            detect_agent_from_argv_and_title(Some("claude.cmd"), None, None),
            Some(AgentKind::ClaudeCode)
        );
        assert_eq!(
            detect_agent_from_argv_and_title(Some("rg.exe"), None, None),
            None
        );
        assert_eq!(
            detect_agent_from_argv_and_title(Some("OpenConsole.exe"), None, None),
            None
        );
    }

    #[test]
    fn detects_node_wrapped_agents_via_argv1() {
        assert_eq!(
            detect_agent_from_argv_and_title(
                Some(r"C:\Program Files\nodejs\node.exe"),
                Some(
                    r"C:\Users\akash\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\cli.js"
                ),
                None,
            ),
            Some(AgentKind::ClaudeCode)
        );
        assert_eq!(
            detect_agent_from_argv_and_title(
                Some("node.exe"),
                Some(r"C:\Users\akash\AppData\Roaming\npm\node_modules\@openai\codex\bin\codex.js"),
                None,
            ),
            Some(AgentKind::Codex)
        );
    }

    #[test]
    fn falls_back_to_osc_title_for_wsl_or_ssh() {
        assert_eq!(
            detect_agent_from_argv_and_title(
                Some("ssh.exe"),
                None,
                Some("✳ Claude Code — Oxidite")
            ),
            Some(AgentKind::ClaudeCode)
        );
        assert_eq!(
            detect_agent_from_argv_and_title(None, None, Some("Antigravity CLI")),
            Some(AgentKind::Antigravity)
        );
    }

    #[test]
    fn ignores_filesystem_paths_containing_agent_words_in_title() {
        assert_eq!(
            detect_agent_from_argv_and_title(None, None, Some(r"C:\Users\devin\Projects\app")),
            None
        );
        assert_eq!(
            detect_agent_from_argv_and_title(None, None, Some("/home/user/codex-clone")),
            None
        );
        assert_eq!(
            detect_agent_from_argv_and_title(None, None, Some("~/cursor/workspace")),
            None
        );
        assert_eq!(
            detect_agent_from_argv_and_title(None, None, Some("OpenAI Codex")),
            Some(AgentKind::Codex)
        );
        assert_eq!(
            detect_agent_from_argv_and_title(None, None, Some("Devin CLI")),
            Some(AgentKind::Devin)
        );
    }

    #[test]
    fn stale_osc_title_ignored_when_local_shell_has_no_opaque_children() {
        let cache = ProcessProbeCache::new(Duration::from_mins(1));
        let self_pid = std::process::id();
        // Current test process has no ssh/wsl child; a lingering "Antigravity CLI" title must not report active.
        assert_eq!(
            cache.detect_for_shell(self_pid, Some("Antigravity CLI")),
            None
        );
    }

    #[test]
    fn extracts_only_first_two_args_dropping_secret_flags() {
        let cmd = r#""C:\Program Files\nodejs\node.exe" "C:\npm\@openai\codex\bin\codex.js" --api-key sk-secret123"#;
        let (a0, a1) = extract_first_two_args(cmd);
        assert_eq!(a0.as_deref(), Some(r"C:\Program Files\nodejs\node.exe"));
        assert_eq!(a1.as_deref(), Some(r"C:\npm\@openai\codex\bin\codex.js"));
    }

    #[test]
    fn plain_cursor_ide_and_unrelated_paths_are_not_agents() {
        // The `cursor` launcher is the IDE, not the agent CLI.
        assert_eq!(
            detect_agent_from_argv_and_title(Some("/usr/bin/cursor"), None, None),
            None
        );
        assert_eq!(
            detect_agent_from_argv_and_title(Some("cursor-agent"), None, None),
            Some(AgentKind::Cursor)
        );
        // Paths that merely contain the words.
        assert_eq!(
            detect_agent_from_argv_and_title(
                Some("node"),
                Some("/home/u/cursor/app/server.js"),
                None
            ),
            None
        );
        assert_eq!(
            detect_agent_from_argv_and_title(
                Some("node"),
                Some("/srv/antigravity-docs/build.js"),
                None
            ),
            None
        );
        assert_eq!(
            detect_agent_from_argv_and_title(
                Some("node"),
                Some("/opt/antigravity/bin/cli.js"),
                None
            ),
            Some(AgentKind::Antigravity)
        );
    }

    #[test]
    fn user_at_host_prompt_titles_are_not_agent_titles() {
        for t in [
            "devin@buildbox:~/src",
            "codex@host",
            "me@devin.example.com",
            "devin.local",
        ] {
            assert_eq!(
                detect_agent_from_argv_and_title(None, None, Some(t)),
                None,
                "{t}"
            );
        }
        assert_eq!(
            detect_agent_from_argv_and_title(None, None, Some("Devin CLI")),
            Some(AgentKind::Devin)
        );
    }

    #[test]
    fn runtime_wrapper_names_cover_linux_comm_and_versions() {
        for n in [
            "node",
            "nodejs",
            "bun",
            "deno",
            "python",
            "python3",
            "python3.12",
            "node.exe",
            "python.exe",
        ] {
            assert!(is_runtime_wrapper(n), "{n}");
        }
        for n in ["nodemon", "pythonista", "bash", "claude", "denoise"] {
            assert!(!is_runtime_wrapper(n), "{n}");
        }
    }

    #[test]
    fn script_token_skips_runtime_flags_but_never_reads_past_the_window() {
        let toks = [
            "node",
            "--inspect",
            "--max-old-space-size=4096",
            "/x/@openai/codex/bin/codex.js",
            "--api-key",
            "sk-1",
        ];
        let (a0, script) = extract_argv0_and_script(toks.into_iter());
        assert_eq!(a0.as_deref(), Some("node"));
        assert_eq!(script.as_deref(), Some("/x/@openai/codex/bin/codex.js"));
        // A script placed beyond the 6-token window is not examined.
        let far = ["node", "-a", "-b", "-c", "-d", "-e", "script.js"];
        assert_eq!(extract_argv0_and_script(far.into_iter()).1, None);
        let (_, s2) = extract_first_two_args(
            r#"node.exe --trace-warnings "C:\npm\@openai\codex\bin\codex.js" --api-key sk-x"#,
        );
        assert_eq!(s2.as_deref(), Some(r"C:\npm\@openai\codex\bin\codex.js"));
    }
}
