/// Concept-card generation and quiz helpers: section formats, prompt builders,
/// upserting `#d/#t/#e/#c/#n` tagged sections, and quiz JSON parsing.
use serde::{Deserialize, Serialize};

/// Section requested from the card context menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenSection {
    All,
    Desc,
    Plain,
    PosExample,
    NegExample,
    Purpose,
    Affect,
    Affected,
}

impl GenSection {
    pub fn label(self) -> &'static str {
        match self {
            GenSection::All => "所有",
            GenSection::Desc => "抽象描述",
            GenSection::Plain => "通俗描述",
            GenSection::PosExample => "正例",
            GenSection::NegExample => "负例",
            GenSection::Purpose => "作用",
            GenSection::Affect => "影响什么",
            GenSection::Affected => "被什么影响",
        }
    }

    /// The header pattern this section's tag lines follow.
    fn pattern(self) -> &'static str {
        match self {
            GenSection::All => unreachable!(),
            GenSection::Desc => "#d {总结标题}",
            GenSection::Plain => "#t {总结标题}",
            GenSection::PosExample => "#e {例子名}(正例)",
            GenSection::NegExample => "#e {例子名}(负例)",
            GenSection::Purpose => "#c 作用 {短标题}",
            GenSection::Affect => "#c influence_to {短标题}",
            GenSection::Affected => "#c influenced_by {短标题}",
        }
    }

    /// All sections except `All`, in the order they appear in the full output.
    pub fn all() -> [GenSection; 7] {
        [
            GenSection::Desc,
            GenSection::Plain,
            GenSection::PosExample,
            GenSection::NegExample,
            GenSection::Purpose,
            GenSection::Affect,
            GenSection::Affected,
        ]
    }
}

/// The role a tag line plays in a card, regardless of its literal title.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SectionKind {
    DescAbstract,
    DescPlain,
    PosExample,
    NegExample,
    Purpose,
    Affect,
    Affected,
}

/// Classify a tag line's role: `#d` = abstract description, `#t` = plain
/// description, `#e` + (负例) = negative example, `#c` keywords = purpose
/// and influence sections.
pub fn section_kind(header: &str) -> Option<SectionKind> {
    let tag = tag_of(header)?;
    let rest = header[tag.len() + 1..].trim_start();
    match tag {
        "#d" => Some(SectionKind::DescAbstract),
        "#t" => Some(SectionKind::DescPlain),
        "#e" => {
            if rest.contains("负例") {
                Some(SectionKind::NegExample)
            } else {
                Some(SectionKind::PosExample)
            }
        }
        "#c" => {
            if rest.starts_with("作用") {
                Some(SectionKind::Purpose)
            } else if rest.starts_with("influence_to") {
                Some(SectionKind::Affect)
            } else if rest.starts_with("influenced_by") {
                Some(SectionKind::Affected)
            } else {
                None
            }
        }
        _ => None,
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
            "概念：{concept_title}\n\n请按顺序完整输出全部七个板块（抽象描述、通俗描述、3 个正例、1~2 个负例、作用、影响什么、被什么影响），格式见系统提示。"
        ),
        _ => format!(
            "概念：{concept_title}\n\n只输出「{}」这一板块，标签行格式为 `{}`。{}",
            section.label(),
            section.pattern(),
            single_output_format(section)
        ),
    };
    (system, user)
}

fn generation_system_prompt() -> String {
    "你是一位概念学习材料写作助手，依据概念学习理论为给定的概念生成学习材料。\n\
     \n\
     【理论基础】\n\
     - 学习材料服务于「判别」：学习者要根据概念的「内涵」（从已见对象提取的共有属性，即判别特征），判断任意一个对象是否属于此概念。\n\
     - 因此材料的核心是：给出概念的判别特征清单，用正例展示特征如何被满足，用负例展示特征如何被违反，让学习者通过对比多个正例抽象出它们的共性（即抽象描述中的特征）。\n\
     \n\
     【输出格式】\n\
     按以下板块输出（「总结标题」「例子名」「短标题」由你自拟）：\n\
     #d {总结标题}\n\
     抽象描述：标签行中的总结标题概括本节内容（如「#d 两个层面」），不要照搬卡片文件名。内容开头写「概念可以通过以下特征来定义：」，随后用 * 逐条罗列判别特征（特征名：说明）。这些特征是判断任意对象是否属于此概念的判别依据，全部满足才归为此概念；不要写成散文式定义。\n\
     \n\
     #t {总结标题}\n\
     通俗描述：用大白话和生活化比喻解释这个概念，让外行也能看懂。标签行同样是概括本节内容的总结标题。\n\
     \n\
     #e {例子名}(正例)\n\
     3 个正例板块，每个板块一个例子：满足全部特征的具体现象，现象之间要有差异，以便学习者通过对比抽象出共性。每个正例先散文描述现象，再写「特征对比」：逐条指出该现象如何满足每个特征。\n\
     \n\
     #e {例子名}(负例)\n\
     1~2 个负例板块，每个板块一个例子：与正例相似但缺失某个关键特征的具体现象，指出它违反了哪些特征。\n\
     \n\
     #c 作用 {短标题}\n\
     学会这个概念后有什么用处（能判别什么问题、指导什么实践）。\n\
     \n\
     #c influence_to {短标题}\n\
     此概念会影响哪些事物，用 * 逐条罗列。\n\
     \n\
     #c influenced_by {短标题}\n\
     哪些事物会影响此概念，用 * 逐条罗列。\n\
     \n\
     注意：卡片文件名可能带有排序用的序号前缀（如「04-lifetime」），正文中请使用概念的自然名称（如「lifetime」或其中文译名）。\n\
     \n\
     【输出要求】\n\
     1. 每个板块简短精炼，优先依据参考资料，参考资料不足时基于自己的知识补充。\n\
     2. 只输出板块标签行和对应内容，不要输出其他说明、总结或 markdown 代码块。\n\
     3. 每个标签行独占一行，内容可占多行。".to_string()
}

fn single_output_format(section: GenSection) -> String {
    match section {
        GenSection::All => String::new(),
        GenSection::PosExample => "共输出 3 个正例板块。".to_string(),
        GenSection::NegExample => "共输出 1~2 个负例板块。".to_string(),
        GenSection::Desc => "第一行标签行必须是 `#d {总结标题}`，标题概括本节内容，不要照搬卡片文件名。".to_string(),
        GenSection::Plain => "第一行标签行必须是 `#t {总结标题}`，标题概括本节内容。".to_string(),
        _ => String::new(),
    }
}

/// Recognised tag prefixes in the card body.
const TAGS: [&str; 5] = ["#d", "#t", "#e", "#c", "#n"];

/// True if `line` starts with one of the recognised tags followed by a space
/// and a non-empty label (labels may start with ASCII, e.g. `#c influence_to`).
fn is_tag_line(line: &str) -> bool {
    TAGS.iter().any(|tag| {
        line.starts_with(tag)
            && line[tag.len()..].starts_with(' ')
            && line[tag.len() + 1..].chars().next().is_some()
    })
}

/// Extract the tag from a tag line (e.g. "#d 抽象描述" -> "#d").
fn tag_of(line: &str) -> Option<&str> {
    TAGS.iter()
        .find(|tag| line.starts_with(*tag) && line[tag.len()..].starts_with(" "))
        .copied()
}

/// Insert a section into `body` after the last section of the same kind, or
/// before the first section of a later kind, or append at the end. Sections
/// are identified by role (SectionKind), not by literal header, so dynamic
/// titles like `#e 图形面积计算(正例)` still group correctly.
pub fn upsert_section(body: &str, header: &str, content: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let content = trim_blank_lines(content);
    let content_lines: Vec<&str> = content.lines().collect();
    let Some(kind) = section_kind(header) else {
        let end = lines.len();
        return splice_insert(lines, header, content_lines, end);
    };

    // Classify the body's sections.
    let mut sections: Vec<(usize, usize, SectionKind)> = Vec::new();
    let mut start: Option<(usize, SectionKind)> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        if is_tag_line(t) {
            if let Some((s, k)) = start.take() {
                sections.push((s, i, k));
            }
            if let Some(k) = section_kind(t) {
                start = Some((i, k));
            }
        }
    }
    if let Some((s, k)) = start.take() {
        sections.push((s, lines.len(), k));
    }

    // After the last same-kind section; else before the first later-kind one;
    // else append at the end.
    let mut insert = lines.len();
    if let Some((_, end, _)) = sections.iter().rev().find(|(_, _, k)| *k == kind) {
        insert = *end;
    } else if let Some((start_idx, _, _)) = sections.iter().find(|(_, _, k)| *k > kind) {
        insert = *start_idx;
    }
    splice_insert(lines, header, content_lines, insert)
}

fn splice_insert(lines: Vec<&str>, header: &str, content_lines: Vec<&str>, insert: usize) -> String {
    let mut out: Vec<String> = Vec::new();
    out.extend(lines[..insert].iter().map(|s| s.to_string()));
    if !out.is_empty() && !out.last().map(|s| s.is_empty()).unwrap_or(true) {
        out.push(String::new());
    }
    out.push(header.to_string());
    out.extend(content_lines.iter().map(|s| s.to_string()));
    if insert < lines.len() {
        out.push(String::new());
        out.extend(lines[insert..].iter().map(|s| s.to_string()));
    }
    while out.last().map(|s| s.is_empty()).unwrap_or(false) {
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
            if !rest.is_empty() {
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
/// Returns the missing human-readable names on failure. Legacy `#n` headers
/// are still accepted alongside the current `#c influence_to/influenced_by`.
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
        } else if t.starts_with("#c influence_to") || t.starts_with("#n 影响什么") {
            has_affect = true;
        } else if t.starts_with("#c influenced_by") || t.starts_with("#n 被什么影响") {
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
    fn upsert_inserts_after_same_kind() {
        let body = "#d 抽象描述\nold\n#e 已有例子(正例)\nex1\n#e 已有例子(负例)\nneg1";
        let out = upsert_section(body, "#e 新例子(正例)", "new pos");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines,
            vec![
                "#d 抽象描述",
                "old",
                "#e 已有例子(正例)",
                "ex1",
                "",
                "#e 新例子(正例)",
                "new pos",
                "",
                "#e 已有例子(负例)",
                "neg1",
            ],
            "{out}"
        );
    }

    #[test]
    fn upsert_inserts_before_later_kind_when_absent() {
        let body = "#e 已有例子(负例)\nneg1";
        let out = upsert_section(body, "#e 新例子(正例)", "new pos");
        assert!(out.starts_with("#e 新例子(正例)\nnew pos\n"), "{out}");
        assert!(out.contains("#e 已有例子(负例)"), "{out}");
    }

    #[test]
    fn upsert_appends_missing_section() {
        let body = "intro\n";
        let out = upsert_section(body, "#c 作用 用途", "plain words");
        assert!(out.ends_with("#c 作用 用途\nplain words"), "{out}");
    }

    #[test]
    fn upsert_unrecognized_header_appends() {
        let body = "intro\n";
        let out = upsert_section(body, "#c something_new 标题", "x");
        assert!(out.ends_with("#c something_new 标题\nx"), "{out}");
    }

    #[test]
    fn upsert_sections_all_at_once() {
        let body = "#d 抽象描述\nold";
        let sections = vec![
            ("#d 抽象描述".to_string(), "new def".to_string()),
            ("#t 通俗描述".to_string(), "plain words".to_string()),
            ("#e 新例子(正例)".to_string(), "pos".to_string()),
            ("#c 作用 用途".to_string(), "use".to_string()),
        ];
        let out = upsert_sections(body, &sections);
        assert!(out.contains("#d 抽象描述\nnew def"), "{out}");
        assert!(out.contains("#t 通俗描述\nplain words"), "{out}");
        assert!(out.contains("#e 新例子(正例)\npos"), "{out}");
        assert!(out.contains("#c 作用 用途\nuse"), "{out}");
        assert!(out.ends_with("#c 作用 用途\nuse"), "{out}");
    }

    #[test]
    fn section_kind_classifies_headers() {
        assert_eq!(section_kind("#d 概念名"), Some(SectionKind::DescAbstract));
        assert_eq!(section_kind("#t 通俗描述"), Some(SectionKind::DescPlain));
        assert_eq!(section_kind("#e 面积(正例)"), Some(SectionKind::PosExample));
        assert_eq!(section_kind("#e 继承(负例)"), Some(SectionKind::NegExample));
        assert_eq!(section_kind("#c 作用 标准化"), Some(SectionKind::Purpose));
        assert_eq!(
            section_kind("#c influence_to 前兆"),
            Some(SectionKind::Affect)
        );
        assert_eq!(
            section_kind("#c influenced_by 后继"),
            Some(SectionKind::Affected)
        );
        assert_eq!(section_kind("#c unknown 标题"), None);
    }

    #[test]
    fn parse_generation_output_with_tags() {
        let text = "好的\n#d 概念名\n特征列表\n#t 通俗描述\n大白话\n#e 面积(正例)\n正例\n#e 继承(负例)\n负例\n#c 作用 用途\n作用\n#c influence_to 前兆\n影响\n#c influenced_by 后继\n被影响";
        let out = parse_generation_output(text);
        assert_eq!(out.len(), 7);
        assert_eq!(out[0], ("#d 概念名".to_string(), "特征列表".to_string()));
        assert_eq!(out[1], ("#t 通俗描述".to_string(), "大白话".to_string()));
        assert_eq!(out[6], ("#c influenced_by 后继".to_string(), "被影响".to_string()));
    }

    #[test]
    fn quiz_ready_detects_missing() {
        let body = "#d 概念名\nx\n#e 面积(正例)\ny\n#c influence_to 前兆\nz";
        assert!(quiz_ready(body).is_err());
        let body2 = "#d 概念名\nx\n#e 继承(负例)\ny\n#c influence_to 前兆\nz\n#c influenced_by 后继\nw";
        assert!(quiz_ready(body2).is_ok());
        let legacy = "#d 概念名\nx\n#e 正例\ny\n#n 影响什么\nz\n#n 被什么影响\nw";
        assert!(quiz_ready(legacy).is_ok());
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
