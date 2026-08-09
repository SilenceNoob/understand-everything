/// Concept-card generation and quiz helpers: section formats, prompt builders,
/// upserting `#d/#t/#e/#n` tagged sections, and quiz JSON parsing.
use serde::{Deserialize, Serialize};

/// Section requested from the card context menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenSection {
    All,
    Desc,
    Plain,
    PosExample,
    NegExample,
    Affect,
    Affected,
}

impl GenSection {
    pub fn label(self) -> &'static str {
        match self {
            GenSection::All => "所有",
            GenSection::Desc => "专业描述",
            GenSection::Plain => "通俗描述",
            GenSection::PosExample => "正面例子",
            GenSection::NegExample => "反面例子",
            GenSection::Affect => "影响什么",
            GenSection::Affected => "被什么影响",
        }
    }

    /// The tag line ("#d 专业描述") used in the card body.
    pub fn header(self) -> String {
        format!("{} {}", self.tag(), self.label())
    }

    /// Tag prefix: #d / #t / #e / #n.
    fn tag(self) -> &'static str {
        match self {
            GenSection::Desc => "#d",
            GenSection::Plain => "#t",
            GenSection::PosExample | GenSection::NegExample => "#e",
            GenSection::Affect | GenSection::Affected => "#n",
            GenSection::All => unreachable!(),
        }
    }

    /// All sections except `All`, in the order they appear in the full output.
    pub fn all() -> [GenSection; 6] {
        [
            GenSection::Desc,
            GenSection::Plain,
            GenSection::PosExample,
            GenSection::NegExample,
            GenSection::Affect,
            GenSection::Affected,
        ]
    }
}

/// Build the (system, user) messages for generating a section of a card.
/// `context` is the BM25/hybrid excerpt block from `rag::service::format_context`.
pub fn generation_messages(
    section: GenSection,
    concept_title: &str,
    context: &str,
) -> (String, String) {
    let system = if context.is_empty() {
        generation_system_prompt()
    } else {
        format!("{context}\n\n{}", generation_system_prompt())
    };
    let user = match section {
        GenSection::All => format!(
            "概念：{concept_title}\n\n请按顺序完整输出全部六个板块的内容。{}",
            all_output_format()
        ),
        _ => format!(
            "概念：{concept_title}\n\n只输出「{}」这一板块。{}",
            section.label(),
            single_output_format(section)
        ),
    };
    (system, user)
}

fn generation_system_prompt() -> String {
    "你是一位擅长概念学习卡片写作的助手。请依据提供的参考资料，为给定的概念输出学习材料。要求：\n\
     1. 每个板块简短精炼，优先依据参考资料，参考资料不足时基于自己的知识补充。\n\
     2. 只输出板块标签行和对应内容，不要输出其他说明、总结或 markdown 代码块。\n\
     3. 标签行和内容必须在同一行开始，内容可占多行。".to_string()
}

fn single_output_format(section: GenSection) -> String {
    format!(
        "格式要求：第一行必须是标签行 `{header}`，下面是该板块的纯文本内容。不要包含其他板块。",
        header = section.header()
    )
}

fn all_output_format() -> String {
    "格式要求：按以下顺序输出，每个板块以标签行开头，后面紧跟内容。不要添加额外说明。\n\n".to_string()
        + &GenSection::all()
            .iter()
            .map(|s| s.header())
            .collect::<Vec<_>>()
            .join("\n")
}

/// Recognised tag prefixes in the card body.
const TAGS: [&str; 4] = ["#d", "#t", "#e", "#n"];

/// True if `line` starts with one of the recognised tags followed by a space.
fn is_tag_line(line: &str) -> bool {
    TAGS.iter().any(|tag| {
        line.starts_with(tag)
            && line[tag.len()..].starts_with(' ')
            && line[tag.len() + 1..].chars().next().map(|c| !c.is_ascii_alphanumeric()) == Some(true)
    })
}

/// Extract the tag from a tag line (e.g. "#d 专业描述" -> "#d").
fn tag_of(line: &str) -> Option<&str> {
    TAGS.iter()
        .find(|tag| line.starts_with(*tag) && line[tag.len()..].starts_with(" "))
        .copied()
}

/// Replace the section with `header` in `body` by `content`, or append it if absent.
/// `header` is e.g. "#d 专业描述".
pub fn upsert_section(body: &str, header: &str, content: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut idx = None;
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start() == header {
            idx = Some(i);
            break;
        }
    }
    let content = trim_blank_lines(content);
    let content_lines: Vec<&str> = content.lines().collect();
    let mut out: Vec<String> = Vec::new();

    if let Some(start) = idx {
        // keep lines before the section
        out.extend(lines[..start].iter().map(|s| s.to_string()));
        // insert the new header and content
        out.push(header.to_string());
        out.extend(content_lines.iter().map(|s| s.to_string()));
        // skip the old header and its body until the next tag line
        let mut i = start + 1;
        while i < lines.len() && !is_tag_line(lines[i].trim_start()) {
            i += 1;
        }
        // keep trailing lines from the next tag onward
        out.extend(lines[i..].iter().map(|s| s.to_string()));
    } else {
        // append
        if !body.is_empty() && !body.ends_with('\n') {
            out.push(body.to_string());
        } else if !body.is_empty() {
            out.push(body.to_string());
        }
        // separate from existing content
        if !out.is_empty() && !out.last().map(|s| s.is_empty()).unwrap_or(true) {
            out.push(String::new());
        }
        out.push(header.to_string());
        out.extend(content_lines.iter().map(|s| s.to_string()));
    }

    // ensure trailing blank line kept only if there is trailing content
    while out.len() > 1 && out.last().map(|s| s.is_empty()).unwrap_or(false) && out[out.len() - 2].is_empty() {
        out.pop();
    }
    out.join("\n")
}

/// Upsert multiple sections parsed from generation output.
pub fn upsert_sections(body: &str, sections: &[(String, String)]) -> String {
    let mut body = body.to_string();
    for (header, content) in sections {
        body = upsert_section(&body, header, content);
    }
    body
}

/// Parse generation output into (header, content) pairs. Tolerates leading/trailing prose.
pub fn parse_generation_output(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(tag) = tag_of(trimmed) {
            let rest = &trimmed[tag.len() + 1..];
            if !rest.is_empty()
                && rest.chars().next().map(|c| !c.is_ascii_alphanumeric()) == Some(true)
            {
                if let Some((h, lines)) = current.take() {
                    out.push((h, trim_blank_lines(&lines.join("\n"))));
                }
                current = Some((line.trim_start().to_string(), Vec::new()));
                continue;
            }
        }
        if let Some((_, ref mut lines)) = current {
            lines.push(line.to_string());
        }
    }
    if let Some((h, lines)) = current.take() {
        out.push((h, trim_blank_lines(&lines.join("\n"))));
    }
    out
}

fn trim_blank_lines(s: &str) -> String {
    let mut lines: Vec<&str> = s.lines().collect();
    while let Some(last) = lines.last() {
        if last.trim().is_empty() {
            lines.pop();
        } else {
            break;
        }
    }
    let mut start = 0;
    while start < lines.len() && lines[start].trim().is_empty() {
        start += 1;
    }
    lines[start..].join("\n")
}

/// Check whether a card body contains the four sections required for a quiz.
/// Returns the missing human-readable names on failure.
pub fn quiz_ready(body: &str) -> Result<(), Vec<String>> {
    let mut has_desc = false;
    let mut has_example = false;
    let mut has_affect = false;
    let mut has_affected = false;

    for line in body.lines() {
        let t = line.trim_start();
        if t.starts_with("#d ") || t.starts_with("#t ") {
            has_desc = true;
        } else if t.starts_with("#e ") {
            has_example = true;
        } else if t.starts_with("#n 影响什么") {
            has_affect = true;
        } else if t.starts_with("#n 被什么影响") {
            has_affected = true;
        }
    }

    let mut missing = Vec::new();
    if !has_desc {
        missing.push("描述（#d/#t）".to_string());
    }
    if !has_example {
        missing.push("例子（#e）".to_string());
    }
    if !has_affect {
        missing.push("影响什么（#n 影响什么）".to_string());
    }
    if !has_affected {
        missing.push("被什么影响（#n 被什么影响）".to_string());
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

/// Quiz data emitted by the model and consumed by the quiz panel.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Quiz {
    pub single: Vec<SingleQuestion>,
    pub multi: Vec<MultiQuestion>,
    pub open: Vec<OpenQuestion>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SingleQuestion {
    pub question: String,
    pub options: Vec<String>,
    pub answer: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MultiQuestion {
    pub question: String,
    pub options: Vec<String>,
    pub answers: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct OpenQuestion {
    pub question: String,
    pub reference_answer: String,
}

/// Build the (system, user) messages for generating a quiz from a card body.
pub fn quiz_generation_messages(body: &str) -> (String, String) {
    let system = format!(
        "你根据下面的概念学习卡片内容出题。要求：\n\
         1. 单选题 3 道，每道 4 个选项，答案为 A/B/C/D 中的一个字母。\n\
         2. 多选题 2 道，每道 4 个选项，答案为字母数组（如 [\"A\",\"C\"]）。\n\
         3. 费曼学习法开放题 1 道，要求学生用自己的话解释概念，并给出标准解答。\n\
         4. 题目要围绕概念理解，不要只是死记硬背。\n\
         5. 必须只输出 JSON，不要 markdown 代码块、不要解释。\n\n\
         卡片内容：\n{}\n\n\
         JSON 格式：\n{}",
        body,
        quiz_json_schema()
    );
    (system, "请出题。".to_string())
}

fn quiz_json_schema() -> String {
    r#"{
  "single": [
    {"question": "...", "options": ["A. ...", "B. ...", "C. ...", "D. ..."], "answer": "A"}
  ],
  "multi": [
    {"question": "...", "options": ["A. ...", "B. ...", "C. ...", "D. ..."], "answers": ["A", "C"]}
  ],
  "open": [
    {"question": "...", "reference_answer": "..."}
  ]
}"#
        .to_string()
}

/// Parse a possibly fenced JSON response into a Quiz.
pub fn parse_quiz(text: &str) -> Result<Quiz, String> {
    let mut text = text.trim();
    if text.starts_with("```json") {
        text = text.strip_prefix("```json").unwrap_or(text).trim();
    } else if text.starts_with("```") {
        text = text.strip_prefix("```").unwrap_or(text).trim();
    }
    if text.ends_with("```") {
        text = text.strip_suffix("```").unwrap_or(text).trim();
    }
    serde_json::from_str(text).map_err(|e| format!("JSON 解析失败: {e}"))
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct GradeResult {
    pub score: i32,
    pub feedback: String,
    pub reference_answer: String,
}

/// Build grading messages. `questions` and `submissions` are in order.
pub fn quiz_grading_messages(
    body: &str,
    questions: &[OpenQuestion],
    submissions: &[String],
) -> (String, String) {
    let system = format!(
        "你根据下面的概念学习卡片内容，为学生的开放题答案评分。\n\
         评分标准：0-10 分，依据回答是否抓住核心概念、是否用学生自己的语言清晰表达。\n\
         输出 JSON 数组，每个元素对应一道题，顺序与输入一致。\n\
         字段：score（整数 0-10）、feedback（简短中文评语）、reference_answer（标准解答）。\n\
         只输出 JSON，不要解释。\n\n\
         卡片内容：\n{}\n\n\
         JSON 格式示例：\n[\n  {{\"score\": 8, \"feedback\": \"...\", \"reference_answer\": \"...\"}}\n]",
        body
    );
    let user = questions
        .iter()
        .zip(submissions.iter())
        .enumerate()
        .map(|(i, (q, a))| format!("{}. 问题：{}\n学生答案：{}", i + 1, q.question, a))
        .collect::<Vec<_>>()
        .join("\n\n");
    (system, user)
}

pub fn parse_grades(text: &str) -> Result<Vec<GradeResult>, String> {
    let mut text = text.trim();
    if text.starts_with("```json") {
        text = text.strip_prefix("```json").unwrap_or(text).trim();
    } else if text.starts_with("```") {
        text = text.strip_prefix("```").unwrap_or(text).trim();
    }
    if text.ends_with("```") {
        text = text.strip_suffix("```").unwrap_or(text).trim();
    }
    serde_json::from_str(text).map_err(|e| format!("评分解析失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_replaces_existing_section() {
        let body = "intro\n#d 专业描述\nold\n#e 正面例子\nexample";
        let out = upsert_section(body, "#d 专业描述", "new def");
        assert!(out.contains("#d 专业描述\nnew def"), "{out}");
        assert!(!out.contains("old"), "{out}");
        assert!(out.contains("#e 正面例子\nexample"), "{out}");
    }

    #[test]
    fn upsert_appends_missing_section() {
        let body = "intro\n";
        let out = upsert_section(body, "#t 通俗描述", "plain words");
        assert!(out.ends_with("#t 通俗描述\nplain words"), "{out}");
    }

    #[test]
    fn upsert_sections_all_at_once() {
        let body = "#d 专业描述\nold";
        let sections = vec![
            ("#d 专业描述".to_string(), "new def".to_string()),
            ("#t 通俗描述".to_string(), "plain".to_string()),
        ];
        let out = upsert_sections(body, &sections);
        assert!(out.contains("#d 专业描述\nnew def"), "{out}");
        assert!(out.contains("#t 通俗描述\nplain"), "{out}");
    }

    #[test]
    fn parse_generation_output_with_tags() {
        let text = "好的\n#d 专业描述\n定义\n\n#t 通俗描述\n比喻\n#e 正面例子\n正例\n#e 反面例子\n反例\n#n 影响什么\n影响\n#n 被什么影响\n被影响";
        let out = parse_generation_output(text);
        assert_eq!(out.len(), 6);
        assert_eq!(out[0], ("#d 专业描述".to_string(), "定义".to_string()));
        assert_eq!(out[5], ("#n 被什么影响".to_string(), "被影响".to_string()));
    }

    #[test]
    fn quiz_ready_detects_missing() {
        let body = "#d 专业描述\nx\n#e 正面例子\ny\n#n 影响什么\nz";
        assert!(quiz_ready(body).is_err());
        let body2 = "#t 通俗描述\nx\n#e 反面例子\ny\n#n 影响什么\nz\n#n 被什么影响\nw";
        assert!(quiz_ready(body2).is_ok());
    }

    #[test]
    fn parse_quiz_fenced_json() {
        let text = "```json\n{\"single\":[{\"question\":\"q\",\"options\":[\"A.a\",\"B.b\",\"C.c\",\"D.d\"],\"answer\":\"A\"}],\"multi\":[],\"open\":[]}\n```";
        let q = parse_quiz(text).unwrap();
        assert_eq!(q.single.len(), 1);
        assert_eq!(q.single[0].answer, "A");
    }

    #[test]
    fn parse_grades_ok() {
        let text = "[{\"score\":7,\"feedback\":\"ok\",\"reference_answer\":\"ref\"}]";
        let g = parse_grades(text).unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].score, 7);
    }
}
