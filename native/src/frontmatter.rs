// YAML Frontmatter 注入
//
// 在转换结果前加一段 YAML 元数据，方便 LLM / 知识库系统识别来源与属性。
//
// ```yaml
// ---
// title: 年度报告
// source: D:\Docs\报告.docx
// type: pdf
// size: 428163
// converted_at: 2026-10-23T19:44:12Z
// ---
// ```

use crate::state::Task;

/// 生成 Frontmatter 块（不含末尾换行，由调用方拼接）
pub fn build(task: &Task) -> String {
    let mut s = String::from("---\n");
    s.push_str(&format!("title: {}\n", yaml_str(&task.name)));
    s.push_str(&format!("source: {}\n", yaml_str(&task.path.display().to_string())));
    s.push_str(&format!("type: {}\n", yaml_str(&task.ext)));
    s.push_str(&format!("size: {}\n", task.size));
    s.push_str(&format!(
        "converted_at: {}\n",
        yaml_str(&now_iso8601())
    ));
    s.push_str("---");
    s
}

/// 把 Frontmatter 注入到 markdown 内容前面
pub fn inject(task: &Task, md: &str) -> String {
    format!("{}\n\n{}", build(task), md)
}

/// YAML 字符串转义：含特殊字符时用双引号包裹
pub fn yaml_str(s: &str) -> String {
    let needs_quote = s.is_empty()
        || s.chars().any(|c| {
            matches!(c, ':' | '#' | '-' | '{' | '}' | '[' | ']' | ',' | '&' | '*'
                | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`' | '\n' | '\r' | '\t')
        })
        || s.trim() != s
        || s.starts_with(' ')
        || s.ends_with(' ');

    if !needs_quote {
        return s.to_string();
    }

    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 当前时间 ISO8601 UTC
fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 简易转换（UTC），够用即可
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, mo, d, h, mi, s
    )
}

/// 天数 → (年, 月, 日)，Howard Hinnant 算法
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
