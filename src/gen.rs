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

/// Card archetype per the learning theory: 概念卡 = 判别模型 (给对象判概念),
/// 知识卡 = 联结模型 (由输入常量推测输出常量).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardType {
    Concept,
    Knowledge,
}

/// Detect a card's archetype from the route-seeded marker line
/// `#c 知识类型 联结模型` (absent -> concept, the legacy default).
pub fn card_type(body: &str) -> CardType {
    for line in body.lines() {
        let t = line.trim_start();
        if t.starts_with("#c 知识类型") && t.contains("联结") {
            return CardType::Knowledge;
        }
    }
    CardType::Concept
}

/// Messages for the first phase of 划选生成子卡片: the model only judges
/// whether the selected text is a concept (判别模型) or knowledge (联结模型),
/// names it, and gives its input/output spaces. The body is generated later
/// by the per-section pipeline (start_generation), so this response stays
/// small and cannot be truncated. `context` is the BM25/hybrid excerpt block
/// (may be empty); `parent_body` is the source card's body.
pub fn subcard_judge_messages(
    parent_title: &str,
    parent_body: &str,
    selected: &str,
    context: &str,
) -> (String, String) {
    let system = "你是一位学习卡片写作助手。用户在一张学习卡片里划选了一段内容，认为其中包含\
        他还不理解的概念或知识。请你判断它的类型、给它命名、并概括其输入输出，\
        卡片正文稍后由另一个助手按标准板块格式生成，你不需要写正文。\n\
         \n\
         【类型判断】\n\
         - 概念（判别模型）：描述「一类事物的判定依据/共有属性」，用于把对象归类（如：所有权、\
         生命周期、熵）。判别特征：可以用「什么现象属于/不属于它」来检验。\n\
         - 知识（联结模型）：描述「两类现象之间的映射/规律」，由输入推测输出（如：所有权转移后\
         原变量失效、热肉须加开水）。判别特征：存在明确的「输入→输出」关系。\n\
         无法判断时默认按概念处理。\n\
         \n\
         【输出格式】\n\
         必须只输出 JSON 对象，不要 markdown 代码块、不要任何解释或前后缀文字：\n\
         {\n\
           \"title\": \"自然名称（不要带序号前缀，不要含 / 、\\\\ 等路径字符）\",\n\
           \"type\": \"concept\" 或 \"knowledge\"，\n\
           \"input\": \"输入空间的现象，一句话概括\",\n\
           \"output\": \"输出空间的结果，一句话概括\",\n\
           \"input_space\": \"输入空间的详细描述，仅联结模型填写，概念卡留空字符串\",\n\
           \"output_space\": \"输出空间的详细描述，仅联结模型填写，概念卡留空字符串\"\n\
         }\n\
         概念卡（判别模型）的 input/output 按标准形式填写：输入=论域内所有现象（可结合划选内容具体化论域），\
         输出=此概念/非此概念。联结模型卡：input/output 用一句话概括，input_space 写清哪些现象属于输入空间\
         （适用对象，含判别特征），output_space 写清输出空间涵盖什么结果。"
        .to_string();
    let system = if context.is_empty() {
        system
    } else {
        format!("{context}\n\n{system}")
    };
    let user = format!(
        "【父卡片标题】{parent_title}\n【父卡片正文】\n{parent_body}\n\n\
         【用户划选的内容】\n{selected}\n\n请判断类型并输出 JSON。"
    );
    (system, user)
}

/// The judged identity of a selected text: title, archetype, and (for
/// knowledge cards) the input/output space summaries.
pub struct SubcardJudge {
    pub title: String,
    pub ctype: CardType,
    pub input: String,
    pub output: String,
    pub input_space: String,
    pub output_space: String,
}

#[derive(Deserialize)]
struct JudgeResp {
    title: String,
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    input: String,
    #[serde(default)]
    output: String,
    #[serde(default)]
    input_space: String,
    #[serde(default)]
    output_space: String,
}

/// Parse the judge response. Tolerates prose around the object: extracts the
/// first `{` .. last `}` span before parsing.
pub fn parse_subcard_judge(text: &str) -> Result<SubcardJudge, String> {
    let t = strip_json_fence(text);
    // Try candidate spans from the last `{` backwards: prose/thinking before
    // the JSON may itself contain braces, and the JSON's own braces are
    // balanced, so first-`{`-to-last-`}` can start on the wrong brace.
    let mut last_err = String::new();
    for (i, _) in t.match_indices('{').collect::<Vec<_>>().into_iter().rev() {
        let cand = &t[i..];
        let cand = match cand.rfind('}') {
            Some(e) => &cand[..=e],
            None => cand,
        };
        match serde_json::from_str::<JudgeResp>(cand) {
            Ok(v) => {
                last_err.clear();
                return parse_judge(v);
            }
            Err(e) => last_err = format!("JSON 解析失败: {e}"),
        }
    }
    if !last_err.is_empty() {
        return Err(last_err);
    }
    Err("JSON 解析失败: 未找到 JSON 对象".to_string())
}

fn parse_judge(v: JudgeResp) -> Result<SubcardJudge, String> {
    let title = v.title.trim().to_string();
    if title.is_empty() {
        return Err("标题为空".to_string());
    }
    let ctype = if v.r#type.contains("联结") || v.r#type == "knowledge" {
        CardType::Knowledge
    } else {
        CardType::Concept
    };
    Ok(SubcardJudge {
        title,
        ctype,
        input: v.input,
        output: v.output,
        input_space: v.input_space,
        output_space: v.output_space,
    })
}



/// Build the (system, user) messages for generating a section of a card.
/// `context` is the BM25/hybrid excerpt block from `rag::service::format_context`.
/// Concept cards explain a 判别模型 (intension + 正负例); knowledge cards
/// explain a 联结模型 (输入/输出空间 + 映射关系 + 归纳实例 + 适用边界).
pub fn generation_messages(
    section: GenSection,
    concept_title: &str,
    context: &str,
    ctype: CardType,
) -> (String, String) {
    let system_prompt = match ctype {
        CardType::Concept => generation_system_prompt(),
        CardType::Knowledge => knowledge_generation_system_prompt(),
    };
    let system = if context.is_empty() {
        system_prompt
    } else {
        format!("{context}\n\n{system_prompt}")
    };
    let what = match ctype {
        CardType::Concept => "概念",
        CardType::Knowledge => "知识（联结模型）",
    };
    let user = match section {
        GenSection::All => format!(
            "{what}：{concept_title}\n\n请按顺序完整输出全部七个板块（抽象描述、通俗描述、3 个正例、1~2 个负例、作用、影响什么、被什么影响），格式见系统提示。"
        ),
        _ => format!(
            "{what}：{concept_title}\n\n只输出「{}」这一板块，标签行格式为 `{}`。{}",
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
     标签行中的总结标题概括本节内容（如「#d 两个层面」），不要照搬卡片文件名。内容直接以「概念可以通过以下特征来定义：」开头，随后用 * 逐条罗列判别特征（特征名：说明）。这些特征是判断任意对象是否属于此概念的判别依据，全部满足才归为此概念；不要写成散文式定义，不要输出「抽象描述：」等栏目前缀。\n\
     \n\
     #t {总结标题}\n\
     通俗描述：用大白话和生活化比喻解释这个概念，让外行也能看懂。标签行同样是概括本节内容的总结标题。\n\
     \n\
     #e {例子名}(正例)\n\
     3 个正例板块，每个板块一个例子：满足全部特征的具体现象，现象之间要有差异，以便学习者通过对比抽象出共性。非代码类概念：先散文描述现象，再写「特征对比」，逐条指出该现象如何满足每个特征。代码相关概念（编程语言特性、API、设计模式等）：先给出例子代码片段，再简要解释这段代码在做什么，最后写「特征对比」，逐条指出该代码如何满足每个特征。\n\
     \n\
     #e {例子名}(负例)\n\
     1~2 个负例板块，每个板块一个例子：与正例相似但缺失某个关键特征的具体现象，指出它违反了哪些特征。代码相关概念用反例代码呈现（通常无法编译或行为不符合预期）：先给出反例代码，再逐条指出它违反了哪些特征；非代码类概念用散文描述现象。\n\
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

/// System prompt for knowledge (联结模型) cards: the card documents the
/// input/output spaces and the mapping, plus 已见 instances (归纳依据) and
/// out-of-extension situations (适用边界, 防判联错配).
fn knowledge_generation_system_prompt() -> String {
    "你是一位知识（联结模型）学习材料写作助手，依据概念学习理论为给定的知识生成学习材料。\n\
     \n\
     【理论基础】\n\
     - 知识 = 联结模型：从「一个概念（变量）的外延」到「另一个概念（变量）的外延」的映射，\
     由已见的「输入常量→输出常量」对应归纳而来；应用时用「输入常量」推测「输出常量」。\n\
     - 材料核心：讲清楚输入空间（哪些现象属于输入空间，是适用对象）、输出空间、映射关系（通用规律），\
     用已见实例展示「输入→输出」的对应，用不适用情形展示输入空间之外的对象（判别条件不足/对象层丢失/判联错配）。\n\
     \n\
     【输出格式】\n\
     按以下板块输出（「总结标题」「例子名」「短标题」由你自拟）：\n\
     #d {总结标题}\n\
     标签行中的总结标题概括本节内容，不要照搬卡片文件名。内容结构：第一行写一句结论式总述（这个知识的映射关系结论），独立成行；\
     随后每个要点独占一行，行首用「==要点名==」高亮标记（要点名按内容自拟，如 ==输入空间==、==映射规律==、==边界情况==），\
     要点名后接该要点的内容。要点要覆盖：输入如何映射到输出的通用规律、何时成立、边界情况如何，\
     让学习者能用它推测未见输入的结果。不要罗列输入/输出空间的完整清单（清单已在「#c 输入输出」板块声明），\
     不要写成散文式长段，不要输出「抽象描述：」等栏目前缀。\n\
     格式示例：\n\
     #d 炖煮水温决定热肉质地：热肉须加开水\n\
     热肉加开水炖煮时，肉质保持松软；若加冷水，热肉表层骤然遇冷收缩，肉质变硬。\n\
     ==输入空间== 关键特征是「肉块已受热（煸炒后）正要加水炖煮」这一状态。\n\
     ==映射规律== 肉越热、水温越低，收缩越剧烈，肉质越硬；水温与肉温接近或更高（开水）时，肉质不收缩、保持软嫩。\n\
     ==边界情况== 若肉尚未受热（如生肉焯水），则用冷水下锅慢慢升温，不属于此知识适用对象；加水量须没过肉块。\n\
     \n\
     #t {总结标题}\n\
     通俗描述：用大白话和生活化比喻解释这个知识是做什么的、什么时候用，让外行也能看懂。\n\
     \n\
     #e {例子名}(正例)\n\
     2~3 个已见实例板块，每个板块一个实例：从输入到输出的具体对应，是归纳此知识的依据。实例之间要有差异。非代码类知识：先散文描述场景，再写「输入→输出」：逐条指出输入常量、输出常量以及如何体现映射关系。代码相关知识（编程语言特性、API、设计模式等）：先给出例子代码片段，再简要解释这段代码中「输入→输出」如何发生，最后写「输入→输出」：逐条指出输入常量、输出常量以及如何体现映射关系。\n\
     \n\
     #e {例子名}(负例)\n\
     1~2 个不适用情形板块：与输入空间相似、但不属于输入空间的现象（或套用此知识会推测出错的情形），\
     指出它缺少输入空间所需的哪些判别特征、为什么不能用此知识处理。代码相关知识用反例代码呈现（通常无法编译或行为不符合预期）：先给出反例代码，再指出它缺少输入空间的哪些判别特征、为什么不能用此知识处理；非代码类知识用散文描述现象。\n\
     \n\
     #c 作用 {短标题}\n\
     学会这个知识后能解决什么实际问题（能推测什么输出）。\n\
     \n\
     #c influence_to {短标题}\n\
     此知识影响/决定了哪些事物，用 * 逐条罗列。\n\
     \n\
     #c influenced_by {短标题}\n\
     哪些事物会影响此知识的成立（前提条件），用 * 逐条罗列。\n\
     \n\
     注意：卡片文件名可能带有排序用的序号前缀（如「04-lifetime」），正文中请使用知识的自然名称。\n\
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
                    out.push((h.clone(), strip_label_echo(&h, trim_blank_lines(&lines.join("\n")))));
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
        out.push((h.clone(), strip_label_echo(&h, trim_blank_lines(&lines.join("\n")))));
    }
    out
}

/// Models sometimes echo the format label from the prompt as the section's
/// first content line ("抽象描述：", "通俗描述："). Strip it so cards don't
/// carry a stray label line before the real content.
fn strip_label_echo(header: &str, content: String) -> String {
    let label = if header.starts_with("#d") {
        "抽象描述："
    } else if header.starts_with("#t") {
        "通俗描述："
    } else {
        return content;
    };
    content.trim_start().strip_prefix(label).unwrap_or(&content).trim_start().to_string()
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

/// A route card as planned by the model: one learning task on the map.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RouteCard {
    /// Unique id within the plan; "root" is reserved for the goal card.
    pub id: String,
    /// Parent card id; None = direct child of the root (goal) card.
    #[serde(default)]
    pub parent: Option<String>,
    pub title: String,
    /// "concept" = 判别模型卡, "knowledge" = 联结模型卡.
    #[serde(rename = "type")]
    pub card_type: String,
    #[serde(default)]
    pub input: String,
    #[serde(default)]
    pub output: String,
    /// Why this card exists in the route (its role in the learning order).
    #[serde(default)]
    pub reason: String,
}

/// The model's route plan: goal analysis (输入/输出空间) plus the card tree.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RoutePlan {
    #[serde(default)]
    pub goal_input: String,
    #[serde(default)]
    pub goal_output: String,
    /// The planner's assessment of the user's grasp of the goal's knowledge
    /// points (已掌握/薄弱/缺失), written to the root card's 用户情况 section.
    /// Never the interview questions themselves. Empty when no diagnosis.
    #[serde(default)]
    pub user_assessment: String,
    pub cards: Vec<RouteCard>,
}

/// Build the (system, user) messages for planning a learning route from a
/// goal. `context` is the BM25/hybrid excerpt block from
/// `rag::service::format_context` (empty when no refs are available);
/// `diagnostics` is the interview transcript from `format_diag_history`
/// (empty when the user skipped the diagnostic phase).
pub fn route_plan_messages(goal: &str, context: &str, diagnostics: &str) -> (String, String) {
    let system = "你是一位学习路线规划助手，依据下面的概念学习理论，把一个学习目标拆解成一张知识卡片学习路线图。\n\
         \n\
         【理论基础】\n\
         - 知识 = 判别模型 + 联结模型。判别模型：根据「内涵（共有属性/判别条件）」把「一个对象」判别为\
         「此概念」或「非此概念」，输出只是标签；联结模型：从「一个概念的外延」到「另一个概念的外延」的映射，\
         用「输入常量」推测「输出常量」。知识 = 联结模型，其输入/输出概念必须先有对应的判别模型。\n\
         - 学习的第一原则是「明确输入输出」：明确知识的输入空间（对象层：学习者要面对的现象集合）和\
         输出空间（学会后要能推测/产出的结果），即明确每个概念的外延。\n\
         - 两种常见的失败：对象层丢失（只背联结模型，无法把现实现象判别到概念下，知识悬空）和\
         判联错配（用错误的判别模型套用联结模型，产生推理谬误）。路线必须让每个概念的判别模型先于\
         使用它的联结模型出现，且每张卡都要写明自己的输入输出。\n\
         - 学习顺序：先学概念（判别模型），再学基于这些概念的联结模型（知识）。拆解树中，\
         「子卡是父卡的前置」：要学会父卡，必须先掌握它的子卡；学习顺序从叶子到根，根卡片（学习目标）最后学。\n\
         \n\
         【因材施教】\n\
         当提供用户诊断记录时，路线必须针对用户的已有知识裁剪（这是本助手的核心职责）：\n\
         - 用户已能正确判别/运用的概念与知识：不单独出卡，也不向下拆它的前置（它已是叶子）。\n\
         - 用户部分掌握、但它是学习目标或必要前置的：出薄卡，reason 注明「快速复习：<具体薄弱点>」。\n\
         - 用户答错、答不出或跳过的：补齐其前置概念卡，reason 注明该薄弱点，后续材料生成应更详细。\n\
         - 诊断记录与路线矛盾时，以诊断记录为准。\n\
         \n\
         【任务】\n\
         1. 明确输入输出：分析学习目标的输入空间（学会后要能判别/处理哪些现象）和输出空间（学会后要能\
         推测/产出什么）。\n\
         2. 递归拆解学习任务（重点：拆成树，不是一条链）：\n\
         - 根卡 = 学习目标本身（最后学）。从根卡开始拆：目标知识（联结模型）涉及的输入概念、输出概念、\
         关键中间概念，全部拆为根卡的直接子卡。一个联结模型至少涉及两个概念（输入+输出），\
         所以树天然应当分叉；若某张卡的子卡只有一个，检查是否是拆解不足（通常应有多个前置）。\n\
         - 对每张新拆出的卡继续递归：掌握它是否还需要更基础的概念/知识？需要就继续拆成它的子卡。\
         一直拆到「用户已掌握」（见诊断记录）或「凭日常经验即可判别」的概念才停止。\n\
         - 卡片总数 8~15 张；每个分支的深度由用户水平决定，不要为了凑数把树压成链。\n\
         - 对偏内隐（判别条件难以言述，如「悲伤」这类）的概念，在 reason 中注明「需大量正反例」。\n\
         \n\
         【输出】\n\
         必须只输出 JSON，不要 markdown 代码块、不要解释：\n\
        {\n\
          \"goal_input\": \"学习目标的输入空间：…\",\n\
          \"goal_output\": \"学习目标的输出空间：…\",\n\
          \"user_assessment\": \"根据诊断记录评估用户对本目标相关知识点的掌握情况（哪些已掌握、哪些薄弱、哪些完全缺失），2~4 句话，只写评估，不要引用题目原文；没有诊断记录时输出空字符串\",\n\
          \"cards\": [\n\
             {\"id\": \"c1\", \"parent\": null, \"title\": \"输入概念 A\", \"type\": \"concept\", \
         \"input\": \"…\", \"output\": \"…\", \"reason\": \"…\"}},\n\
             {\"id\": \"c2\", \"parent\": null, \"title\": \"输出概念 B\", \"type\": \"concept\", \
         \"input\": \"…\", \"output\": \"…\", \"reason\": \"…\"}},\n\
             {\"id\": \"c3\", \"parent\": \"c1\", \"title\": \"A 的更基础前置\", \"type\": \"concept\", \
         \"input\": \"…\", \"output\": \"…\", \"reason\": \"…\"}}\n\
           ]\n\
        }\n\
         要求：id 唯一且不能是 \"root\"；parent 必须是已出现的卡片 id 或 null（null = 根卡片之子）；\
         type 只能是 \"concept\" 或 \"knowledge\"；卡片标题是概念/知识的自然名称，不要带序号前缀，\
         也不要包含 / 、\\ 等路径字符；输入输出要具体到能指导后续生成学习材料；\
         cards 不要包含学习目标本身（根卡由程序创建，只列出根卡的子卡及更深层卡片）。"
            .to_string();
    let mut user = if context.is_empty() {
        format!("学习目标：{goal}\n\n没有参考资料，请基于自己的知识规划。")
    } else {
        format!("学习目标：{goal}\n\n参考资料（供规划时参考，可能不完整）：\n{context}")
    };
    if !diagnostics.is_empty() {
        user.push_str(&format!("\n\n【用户情况】（诊断记录，已标注答对/答错）\n{diagnostics}"));
    }
    (system, user)
}

/// Build the messages that name a brand-new map from the user's learning
/// goal. The model must reply with a short filename only (no extension, no
/// path separators); the caller sanitizes and unique-ifies it.
pub fn map_name_messages(goal: &str) -> (String, String) {
    let system = "你是一个命名助手。根据学习目标给出一个简短、贴切的中文文件名（2~8 个字，不含扩展名，\
        不含 / \\ 等路径分隔符和标点）。只输出名字本身，不要任何解释、引号或换行。"
        .to_string();
    let user = format!("学习目标：{goal}\n\n请给出这个学习路线的文件名：");
    (system, user)
}

/// One round of the adaptive diagnostic interview: a question probing one
/// concept's 判别模型 or one knowledge's 联结模型.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DiagQuestion {
    /// "single" | "multi" | "open"
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub question: String,
    /// Options with letter prefixes ("A. …"), single/multi only.
    #[serde(default)]
    pub options: Vec<String>,
    /// Normalized correct answer letters: ["A"] single, ["A","C"] multi.
    #[serde(default)]
    pub answer: Vec<String>,
    /// Reference answer for open questions (and context for the grader).
    #[serde(default)]
    pub reference_answer: String,
    /// The concept/knowledge this question probes.
    #[serde(default)]
    pub target: String,
}

/// The model's next move in the adaptive interview.
pub enum DiagStep {
    Question(DiagQuestion),
    /// Interview over; the summary assesses the user's mastery.
    Done(String),
}

/// Build the (system, user) messages for the next adaptive diagnostic round.
/// `history` holds prior rounds (question + user answer, oldest first);
/// empty = first question. `context` is the BM25 excerpt block.
pub fn diagnostic_messages(
    goal: &str,
    context: &str,
    history: &[(DiagQuestion, String)],
) -> (String, String) {
    let system = "你是一位学习诊断面试官，依据概念学习理论，通过一次一道题探明用户已有的知识，\
为后续规划个性化学习路线做准备。\n\
         \n\
         【理论基础】\n\
         - 学会 = 建构概念的判别模型（根据内涵把对象判别为此概念/非此概念）+ 知识的联结模型\
（由输入常量推测输出常量）。\n\
         - 因此诊断的不是「用户会不会」，而是：能否把具体现象正确判别到概念下（判别模型），\
能否由具体输入推测正确输出（联结模型）。\n\
         \n\
         【出题要求】\n\
         - 严禁自我报告题（如「你了解 X 吗」「你学过 X 吗」）。必须给出具体现象/案例让用户判别或推测：\n\
         · 测判别模型（单选/多选）：给出具体对象（可含易混负例），让用户判断它是否属于某概念、\
为什么（选项里给出理由或现象描述）。\n\
         · 测联结模型（单选/多选/主观）：给出具体的输入常量，让用户选择/写出输出常量或处理方式。\n\
         · 主观题（open）：让用户用自己的话解释或推测，探测理解深度。\n\
         - 一次只出一道题。根据用户前面的作答自适应：\n\
         · 前置概念答错/答不出 → 下一题往更基础的概念挖，不要问更高阶的。\n\
         · 答对 → 向学习目标推进。\n\
         · 已探明的概念/知识不要重复问。\n\
         - 覆盖顺序：目标的前置基础概念 → 目标知识本身。\n\
         - 当信息足够判断用户已具备/缺失哪些判别模型与联结模型时，停止出题并输出掌握情况摘要。\
最多再出 6 题。\n\
         \n\
         【输出】只输出 JSON，不要 markdown 代码块、不要解释：\n\
         继续出题：{\"done\": false, \"kind\": \"single\", \"question\": \"...\", \
\"options\": [\"A. ...\", \"B. ...\", \"C. ...\", \"D. ...\"], \"answer\": \"A\", \
\"reference_answer\": \"...\", \"target\": \"本题探测的概念/知识\"}\n\
         多选：kind 为 \"multi\"，answer 为 [\"A\",\"C\"]；主观题：kind 为 \"open\"，无 options、\
answer 可省略，给出 reference_answer。\n\
         结束诊断：{\"done\": true, \"summary\": \"已掌握：…；未掌握：…；薄弱点：…\"}"
            .to_string();
    let mut user = format!("学习目标：{goal}\n");
    if !context.is_empty() {
        user.push_str(&format!("\n参考资料（可能不完整）：\n{context}\n"));
    }
    if history.is_empty() {
        user.push_str("\n这是第一道题。");
    } else {
        user.push_str(&format!(
            "\n前面的问答记录：\n{}\n请根据作答情况出下一道题，或（信息足够时）输出 done 结束诊断。",
            format_diag_history(history)
        ));
    }
    (system, user)
}

/// The answer marker recorded when the user picks the 我不知道 escape hatch
/// on a choice question; annotated as such instead of being judged 答对/答错.
pub const DIAG_UNKNOWN: &str = "我不知道";

/// Render the interview transcript: each round with the question, options,
/// the user's answer and a 答对/答错 annotation (single/multi judged against
/// the stored answer; open questions are left for the model to judge against
/// `reference_answer`). A `DIAG_UNKNOWN` answer is annotated 不知道 (no
/// letter extraction, never 答错), which both the interviewer and the route
/// planner treat as 答不出/缺失.
pub fn format_diag_history(history: &[(DiagQuestion, String)]) -> String {
    let mut out = String::new();
    for (i, (q, user_ans)) in history.iter().enumerate() {
        out.push_str(&format!("Q{}（{}", i + 1, q.kind));
        if !q.target.is_empty() {
            out.push_str(&format!("，探测：{}", q.target));
        }
        out.push_str("）：");
        out.push_str(&q.question);
        if !q.options.is_empty() {
            out.push_str("\n选项：");
            out.push_str(&q.options.join("；"));
        }
        out.push_str(&format!("\n用户答案：{user_ans}"));
        match q.kind.as_str() {
            "single" | "multi" if user_ans == DIAG_UNKNOWN => {
                out.push_str(&format!(
                    "（标准答案：{}，不知道）",
                    q.answer.join(",")
                ));
            }
            "single" | "multi" => {
                let ua: Vec<String> = user_ans
                    .chars()
                    .filter(|c| c.is_ascii_uppercase())
                    .map(|c| c.to_string())
                    .collect();
                let ok = !ua.is_empty() && ua == q.answer;
                out.push_str(&format!(
                    "（标准答案：{}，{}）",
                    q.answer.join(","),
                    if ok { "答对" } else { "答错" }
                ));
            }
            _ => {
                if !q.reference_answer.is_empty() {
                    out.push_str(&format!("（参考解：{}，请对照评判）", q.reference_answer));
                }
            }
        }
        if i + 1 < history.len() {
            out.push('\n');
        }
    }
    out
}

/// Parse the model's next interview step (fenced JSON tolerated). Choices
/// are validated (kind, option count, answer letters).
pub fn parse_diag_step(text: &str) -> Result<DiagStep, String> {
    let v: serde_json::Value = serde_json::from_str(strip_json_fence(text))
        .map_err(|e| format!("JSON 解析失败: {e}"))?;
    if v.get("done").and_then(|d| d.as_bool()) == Some(true) {
        let summary = v
            .get("summary")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string();
        return Ok(DiagStep::Done(summary));
    }
    let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or_default().to_string();
    if !matches!(kind.as_str(), "single" | "multi" | "open") {
        return Err(format!("未知题型：{kind}"));
    }
    let question = v
        .get("question")
        .and_then(|q| q.as_str())
        .unwrap_or_default()
        .to_string();
    if question.is_empty() {
        return Err("诊断题缺少 question 字段".to_string());
    }
    let mut q = DiagQuestion {
        kind,
        question,
        ..Default::default()
    };
    if let Some(opts) = v.get("options").and_then(|o| o.as_array()) {
        q.options = opts
            .iter()
            .filter_map(|o| o.as_str().map(|s| s.to_string()))
            .collect();
    }
    if q.kind != "open" {
        if q.options.len() < 2 || q.options.len() > 4 {
            return Err(format!("选择题选项数必须为 2~4，收到 {}", q.options.len()));
        }
        q.answer = normalize_answer_letters(&v.get("answer"));
        if q.answer.is_empty() {
            return Err("选择题缺少 answer 字段".to_string());
        }
    }
    q.reference_answer = v
        .get("reference_answer")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    q.target = v.get("target").and_then(|s| s.as_str()).unwrap_or_default().to_string();
    Ok(DiagStep::Question(q))
}

/// Extract uppercase answer letters from a JSON string ("A") or array
/// (["A","C"]), sorted and deduped.
fn normalize_answer_letters(v: &Option<&serde_json::Value>) -> Vec<String> {
    let Some(v) = v else { return Vec::new() };
    let mut out = Vec::new();
    match v {
        serde_json::Value::String(s) => {
            out.extend(s.chars().filter(|c| c.is_ascii_uppercase()).map(|c| c.to_string()))
        }
        serde_json::Value::Array(a) => {
            for item in a {
                if let Some(s) = item.as_str() {
                    out.extend(
                        s.chars()
                            .filter(|c| c.is_ascii_uppercase())
                            .map(|c| c.to_string()),
                    );
                }
            }
        }
        _ => {}
    }
    out.sort();
    out.dedup();
    out
}

/// Strip a possibly fenced JSON response (fences may sit anywhere, with
/// leading/trailing prose like "好的" from the model).
fn strip_json_fence(text: &str) -> &str {
    let mut t = text.trim();
    if let Some(start) = t.find("```") {
        let rest = &t[start + 3..];
        t = rest.strip_prefix("json").unwrap_or(rest).trim_start();
    }
    if let Some(end) = t.rfind("```") {
        t = &t[..end];
    }
    t.trim()
}

/// Parse and validate a route plan. Unknown/missing parents are re-attached
/// to the root; cycles are broken the same way. Type strings are normalized
/// to "concept"/"knowledge".
pub fn parse_route_plan(text: &str) -> Result<RoutePlan, String> {
    let t = strip_json_fence(text);
    // Try candidate spans from the last `{` backwards: prose/thinking before
    // the JSON may itself contain braces, and the JSON's own braces are
    // balanced, so first-`{`-to-last-`}` can start on the wrong brace.
    let mut last_err = String::new();
    let mut plan: Option<RoutePlan> = None;
    for (i, _) in t.match_indices('{').collect::<Vec<_>>().into_iter().rev() {
        let cand = &t[i..];
        let cand = match cand.rfind('}') {
            Some(e) => &cand[..=e],
            None => cand,
        };
        match serde_json::from_str::<RoutePlan>(cand) {
            Ok(p) => {
                plan = Some(p);
                break;
            }
            Err(e) => last_err = format!("JSON 解析失败: {e}"),
        }
    }
    let Some(mut plan) = plan else {
        if !last_err.is_empty() {
            return Err(last_err);
        }
        return Err("JSON 解析失败: 未找到 JSON 对象".to_string());
    };
    if plan.cards.is_empty() {
        return Err("路线为空（没有规划出任何卡片）".to_string());
    }
    if plan.cards.len() > 20 {
        return Err(format!("卡片过多（{} 张，上限 20）", plan.cards.len()));
    }
    let mut seen = std::collections::HashSet::new();
    for c in &plan.cards {
        if c.id.is_empty() || c.id == "root" {
            return Err(format!("卡片 id 非法：{c:?}"));
        }
        if !seen.insert(c.id.clone()) {
            return Err(format!("卡片 id 重复：{}", c.id));
        }
    }
    let ids: std::collections::HashSet<&str> = seen.iter().map(|s| s.as_str()).collect();
    for c in &mut plan.cards {
        if c.title.trim().is_empty() {
            return Err("存在标题为空的卡片".to_string());
        }
        if let Some(p) = &c.parent {
            if !ids.contains(p.as_str()) {
                c.parent = None;
            }
        }
        c.card_type = if c.card_type.contains("联结") || c.card_type == "knowledge" {
            "knowledge".to_string()
        } else {
            "concept".to_string()
        };
    }
    // Break cycles: a card whose parent chain loops back to itself roots.
    for i in 0..plan.cards.len() {
        let mut cur = i;
        let mut steps = 0;
        while let Some(p) = plan.cards[cur]
            .parent
            .as_deref()
            .and_then(|pid| plan.cards.iter().position(|c| c.id == pid))
        {
            if p == i || steps >= plan.cards.len() {
                plan.cards[i].parent = None;
                break;
            }
            cur = p;
            steps += 1;
        }
    }
    Ok(plan)
}

/// Drop plan cards whose title equals the goal itself (planners sometimes
/// list the root as a card — "根卡 = 学习目标本身" — even though the root is
/// created by the program). Their children re-attach to the root (parent
/// None = 根卡之子, the same convention `parse_route_plan` uses for unknown
/// parents).
pub fn drop_goal_duplicates(plan: &mut RoutePlan, goal: &str) {
    let goal = goal.trim();
    if goal.is_empty() {
        return;
    }
    let dup_ids: std::collections::HashSet<String> = plan
        .cards
        .iter()
        .filter(|c| c.title.trim() == goal)
        .map(|c| c.id.clone())
        .collect();
    if dup_ids.is_empty() {
        return;
    }
    plan.cards.retain(|c| !dup_ids.contains(&c.id));
    for c in &mut plan.cards {
        if c.parent.as_ref().is_some_and(|p| dup_ids.contains(p)) {
            c.parent = None;
        }
    }
}

/// Strip a "NN-" order prefix from a card title or file stem
/// ("01-实体（Entity）" -> "实体（Entity）"); anything else passes through.
pub fn strip_order_prefix(s: &str) -> &str {
    match s.split_once('-') {
        Some((p, rest)) if p.len() <= 2 && !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
            rest
        }
        _ => s,
    }
}

/// Learning order for route cards: post-order DFS over the parent tree
/// (leaves first, since 子卡是父卡的前置). Parents were validated/re-rooted
/// by `parse_route_plan`, but unreachable cards are appended defensively.
pub fn learning_order(cards: &[RouteCard]) -> Vec<usize> {
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); cards.len()];
    let mut roots = Vec::new();
    for (i, c) in cards.iter().enumerate() {
        match &c.parent {
            Some(p) => {
                if let Some(pi) = cards.iter().position(|c| &c.id == p) {
                    children[pi].push(i);
                } else {
                    roots.push(i);
                }
            }
            None => roots.push(i),
        }
    }
    fn dfs(i: usize, children: &[Vec<usize>], out: &mut Vec<usize>) {
        for &c in &children[i] {
            dfs(c, children, out);
        }
        out.push(i);
    }
    let mut out = Vec::with_capacity(cards.len());
    for &r in &roots {
        dfs(r, &children, &mut out);
    }
    for i in 0..cards.len() {
        if !out.contains(&i) {
            out.push(i);
        }
    }
    out
}

/// Match a planned card `title` against existing card files (rel paths,
/// e.g. "cards/xx/01-实体（Entity）.md"), ignoring "NN-" order prefixes.
/// Exact match on the stripped stem; no fuzzy matching (v1).
pub fn match_card_path(existing: &[String], title: &str) -> Option<String> {
    let want = strip_order_prefix(title.trim());
    existing
        .iter()
        .find(|p| {
            std::path::Path::new(p)
                .file_stem()
                .map(|s| s.to_string_lossy())
                .is_some_and(|s| strip_order_prefix(&s) == want)
        })
        .cloned()
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
/// Concept cards test 判别能力 (给对象判概念 + 举新例); knowledge cards test
/// 联结模型应用 (由输入推测输出 + 适用范围判别).
pub fn quiz_generation_messages(body: &str, ctype: CardType) -> (String, String) {
    let extra = match ctype {
        CardType::Concept => String::new(),
        CardType::Knowledge => {
            "题目围绕知识应用：给出具体的输入常量（现象）选择正确的输出常量（推测结果），\
或判断现象是否属于该知识的输入空间（适用范围），不要只考背诵。"
                .to_string()
        }
    };
    let system = format!(
        "你根据下面的知识卡片内容出题。要求：\n\
         1. 单选题 3 道，每道 4 个选项，答案为 A/B/C/D 中的一个字母。\n\
         2. 多选题 2 道，每道 4 个选项，答案为字母数组（如 [\"A\",\"C\"]）。\n\
         3. 费曼学习法开放题 1 道，要求学生用自己的话解释知识，并给出标准解答。\n\
         4. 题目要围绕知识理解，不要只是死记硬背。{extra}\n\
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
    fn parse_strips_label_echo_from_d_and_t() {
        let text = "#d 抽象描述\n抽象描述：\n输入空间：\n* A\n#t 通俗描述\n通俗描述：用大白话\n#e 例子(正例)\n输入：不应被剥\n";
        let out = parse_generation_output(text);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].0, "#d 抽象描述");
        assert_eq!(out[0].1, "输入空间：\n* A");
        assert_eq!(out[1].0, "#t 通俗描述");
        assert_eq!(out[1].1, "用大白话");
        // inline echo on the same line is stripped too; #e content is untouched
        assert_eq!(out[2].1, "输入：不应被剥");
        // no echo -> content unchanged
        let clean = parse_generation_output("#d 标题\n概念可以通过以下特征来定义：\nx");
        assert_eq!(clean[0].1, "概念可以通过以下特征来定义：\nx");
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

    #[test]
    fn parse_route_plan_ok() {
        let text = r#"{
  "goal_input": "任意物体",
  "goal_output": "是否能浮起来",
  "cards": [
    {"id": "c1", "parent": null, "title": "浮力", "type": "concept", "input": "水中的物体", "output": "浮力/非浮力", "reason": "浮力概念"},
    {"id": "c2", "parent": "c1", "title": "浮力定律", "type": "knowledge", "input": "浸入液体中的物体", "output": "浮力大小", "reason": "联结模型"}
  ]
}"#;
        let plan = parse_route_plan(text).unwrap();
        assert_eq!(plan.cards.len(), 2);
        assert_eq!(plan.goal_input, "任意物体");
        assert_eq!(plan.cards[1].parent.as_deref(), Some("c1"));
        assert_eq!(plan.cards[0].card_type, "concept");
        assert_eq!(plan.cards[1].card_type, "knowledge");
    }

    #[test]
    fn parse_route_plan_fenced() {
        let text = "好的\n```json\n{\"cards\":[{\"id\":\"c1\",\"title\":\"T\",\"type\":\"联结模型\"}]}\n```";
        let plan = parse_route_plan(text).unwrap();
        assert_eq!(plan.cards[0].card_type, "knowledge");
    }

    #[test]
    fn parse_route_plan_repairs_and_rejects() {
        // unknown parent re-attached to root, cycle broken
        let text = r#"{"cards":[
            {"id":"c1","title":"A","type":"concept","parent":"ghost"},
            {"id":"c2","title":"B","type":"concept","parent":"c3"},
            {"id":"c3","title":"C","type":"concept","parent":"c2"}
        ]}"#;
        let plan = parse_route_plan(text).unwrap();
        assert_eq!(plan.cards[0].parent, None);
        assert_eq!(plan.cards[1].parent, None);
        assert_eq!(plan.cards[2].parent, Some("c2".to_string()));
        // empty / duplicate-id / root-id rejected
        assert!(parse_route_plan(r#"{"cards":[]}"#).is_err());
        assert!(parse_route_plan(r#"{"cards":[{"id":"a","title":"x","type":"c"},{"id":"a","title":"y","type":"c"}]}"#).is_err());
        assert!(parse_route_plan(r#"{"cards":[{"id":"root","title":"x","type":"c"}]}"#).is_err());
    }

    #[test]
    fn parse_route_plan_tolerates_prose_and_fences() {
        let ok = r#"{"goal_input":"i","goal_output":"o","cards":[{"id":"c1","title":"T","type":"concept"}]}"#;
        assert!(parse_route_plan(ok).is_ok());
        // prose prefix/suffix
        let prose = format!("好的，以下是规划：\n{ok}\n希望有帮助。");
        assert!(parse_route_plan(&prose).is_ok());
        // fenced
        let fenced = format!("```json\n{ok}\n```");
        assert!(parse_route_plan(&fenced).is_ok());
        // prose containing braces before the JSON (thinking text with `{}`)
        let bracy = format!("好的，我在思考{{拆解}}依据：\n{ok}\n——以上是思考。");
        assert_eq!(parse_route_plan(&bracy).unwrap().cards[0].id, "c1");
        // truncated mid-object must still fail with the parse error
        assert!(parse_route_plan(r#"好的，{思考}"#).is_err());
    }

    #[test]
    fn card_type_detects_marker() {
        assert_eq!(card_type("#d 学习目标\nx"), CardType::Concept);
        assert_eq!(card_type("#c 知识类型 联结模型\nx"), CardType::Knowledge);
        assert_eq!(card_type("#c 知识类型 概念\nx"), CardType::Concept);
    }

    #[test]
    fn parse_diag_step_question_and_done() {
        let text = r#"{"done": false, "kind": "single", "question": "下面哪个是负例？", "options": ["A. 苹果", "B. 香蕉", "C. 石头"], "answer": "C", "target": "水果判别"}"#;
        match parse_diag_step(text).unwrap() {
            DiagStep::Question(q) => {
                assert_eq!(q.kind, "single");
                assert_eq!(q.options.len(), 3);
                assert_eq!(q.answer, vec!["C".to_string()]);
                assert_eq!(q.target, "水果判别");
            }
            _ => panic!("expected question"),
        }
        // multi answer array, normalized+sorted
        let text = r#"{"kind": "multi", "question": "q", "options": ["A. a", "B. b", "C. c", "D. d"], "answer": ["C", "A"]}"#;
        match parse_diag_step(text).unwrap() {
            DiagStep::Question(q) => assert_eq!(q.answer, vec!["A".to_string(), "C".to_string()]),
            _ => panic!("expected question"),
        }
        // done
        match parse_diag_step(r#"{"done": true, "summary": "已掌握：水果"}"#).unwrap() {
            DiagStep::Done(s) => assert_eq!(s, "已掌握：水果"),
            _ => panic!("expected done"),
        }
        // fenced + prose
        match parse_diag_step("好的\n```json\n{\"kind\":\"open\",\"question\":\"解释\",\"reference_answer\":\"r\"}\n```").unwrap() {
            DiagStep::Question(q) => {
                assert_eq!(q.kind, "open");
                assert_eq!(q.reference_answer, "r");
            }
            _ => panic!("expected question"),
        }
        // invalid: bad kind, bad option count, missing answer
        assert!(parse_diag_step(r#"{"kind":"x","question":"q"}"#).is_err());
        assert!(parse_diag_step(r#"{"kind":"single","question":"q","options":["A. a"],"answer":"A"}"#).is_err());
        assert!(parse_diag_step(r#"{"kind":"single","question":"q","options":["A. a","B. b"]}"#).is_err());
    }

    #[test]
    fn format_diag_history_annotates_correctness() {
        let q1 = DiagQuestion {
            kind: "single".to_string(),
            question: "石头是苹果吗？".to_string(),
            options: vec!["A. 是".to_string(), "B. 不是".to_string()],
            answer: vec!["B".to_string()],
            target: "苹果判别".to_string(),
            ..Default::default()
        };
        let q2 = DiagQuestion {
            kind: "open".to_string(),
            question: "为什么？".to_string(),
            reference_answer: "因为石头不可食用".to_string(),
            ..Default::default()
        };
        let h = vec![(q1, "A".to_string()), (q2, "因为石头没有苹果的内涵".to_string())];
        let s = format_diag_history(&h);
        assert!(s.contains("探测：苹果判别"), "{s}");
        assert!(s.contains("用户答案：A（标准答案：B，答错）"), "{s}");
        assert!(s.contains("参考解：因为石头不可食用"), "{s}");
    }

    #[test]
    fn format_diag_history_marks_unknown() {
        let q = DiagQuestion {
            kind: "single".to_string(),
            question: "石头是苹果吗？".to_string(),
            options: vec!["A. 是".to_string(), "B. 不是".to_string()],
            answer: vec!["B".to_string()],
            target: "苹果判别".to_string(),
            ..Default::default()
        };
        let h = vec![(q, DIAG_UNKNOWN.to_string())];
        let s = format_diag_history(&h);
        assert!(s.contains("（标准答案：B，不知道）"), "{s}");
        assert!(!s.contains("答错"), "{s}");
        assert!(!s.contains("答对"), "{s}");
    }

    #[test]
    fn route_plan_messages_includes_diagnostics() {
        let (system, user) = route_plan_messages("目标", "", "Q1…答对");
        assert!(system.contains("因材施教"));
        assert!(system.contains("user_assessment"), "{system}");
        assert!(user.contains("【用户情况】"), "{user}");
        let (_, user2) = route_plan_messages("目标", "", "");
        assert!(!user2.contains("用户情况"));
    }

    #[test]
    fn parse_route_plan_assessment_defaults_empty() {
        let plan = parse_route_plan(r#"{"goal_input":"i","goal_output":"o","cards":[{"id":"c1","title":"T","type":"concept"}]}"#).unwrap();
        assert_eq!(plan.user_assessment, "");
        assert_eq!(plan.goal_input, "i");
    }

    #[test]
    fn knowledge_prompt_d_specs_highlighted_point_lines() {
        let (system, _) = generation_messages(GenSection::Desc, "炖煮水温", "", CardType::Knowledge);
        assert!(system.contains("==映射规律=="), "{system}");
        assert!(system.contains("==输入空间=="), "{system}");
        assert!(system.contains("==边界情况=="), "{system}");
        assert!(system.contains("抽象描述："), "the banned label echo is spelled out to avoid it");
    }

    #[test]
    fn parse_subcard_judge_tolerates_prose_and_fences() {
        let ok = r##"{"title":"所有权转移","type":"concept","input":"","output":""}"##;
        let j = parse_subcard_judge(ok).unwrap();
        assert_eq!(j.title, "所有权转移");
        assert_eq!(j.ctype, CardType::Concept);
        // prose prefix/suffix around the object
        let prose = format!("好的，以下是判断结果：\n{ok}\n希望有帮助。");
        let j = parse_subcard_judge(&prose).unwrap();
        assert_eq!(j.title, "所有权转移");
        // fenced
        let fenced = format!("```json\n{ok}\n```");
        assert!(parse_subcard_judge(&fenced).is_ok());
        // knowledge type names: "knowledge" or Chinese "联结模型"
        let kn = r##"{"title":"热肉须加开水","type":"knowledge","input":"热肉正要加水炖煮","output":"肉质保持软嫩"}"##;
        let j = parse_subcard_judge(kn).unwrap();
        assert_eq!(j.ctype, CardType::Knowledge);
        assert_eq!(j.input, "热肉正要加水炖煮");
        assert_eq!(j.output, "肉质保持软嫩");
        assert_eq!(j.input_space, "");
        assert_eq!(j.output_space, "");
        let kn2 = r##"{"title":"热肉须加开水","type":"联结模型"}"##;
        assert_eq!(parse_subcard_judge(kn2).unwrap().ctype, CardType::Knowledge);
        // knowledge with space descriptions
        let kn3 = r##"{"title":"热肉须加开水","type":"knowledge","input":"热肉正要加水炖煮","output":"肉质保持软嫩","input_space":"肉块已受热正要加水炖煮的状态","output_space":"肉质是否软嫩"}"##;
        let j = parse_subcard_judge(kn3).unwrap();
        assert_eq!(j.input_space, "肉块已受热正要加水炖煮的状态");
        assert_eq!(j.output_space, "肉质是否软嫩");
        // errors
        assert!(parse_subcard_judge(r#"{"title":"","type":"concept"}"#).is_err());
        assert!(parse_subcard_judge("not json").is_err());
        // prose containing braces before the JSON (thinking text with `{}`)
        let bracy = format!("好的，我在思考{{判定}}依据：\n{ok}\n——以上是思考。");
        assert_eq!(parse_subcard_judge(&bracy).unwrap().title, "所有权转移");
        // truncated mid-object must still fail with the parse error
        assert!(parse_subcard_judge(r#"好的，{思考}"#).is_err());
    }

    #[test]
    fn subcard_judge_messages_injects_context_and_selection() {
        let (system, user) = subcard_judge_messages("父卡", "父正文", "划选文字", "参考资料");
        assert!(system.contains("参考资料"), "{system}");
        assert!(system.contains("type"), "{system}");
        assert!(system.contains("input_space"), "{system}");
        assert!(system.contains("output_space"), "{system}");
        assert!(user.contains("父正文"), "{user}");
        assert!(user.contains("划选文字"), "{user}");
        let (system2, _) = subcard_judge_messages("父卡", "", "x", "");
        assert!(!system2.contains("参考资料"), "{system2}");
    }

    #[test]
    fn strip_order_prefix_strips_only_numeric_prefixes() {
        assert_eq!(strip_order_prefix("01-实体（Entity）"), "实体（Entity）");
        assert_eq!(strip_order_prefix("实体（Entity）"), "实体（Entity）");
        assert_eq!(strip_order_prefix("2024财报"), "2024财报");
    }

    #[test]
    fn learning_order_leaves_first() {
        let card = |id: &str, parent: Option<&str>| RouteCard {
            id: id.to_string(),
            parent: parent.map(|s| s.to_string()),
            title: id.to_string(),
            card_type: "concept".to_string(),
            ..Default::default()
        };
        // chain: c1 -> c2 -> c3 (c2's child c3 is the deepest leaf)
        let chain = vec![card("c1", None), card("c2", Some("c1")), card("c3", Some("c2"))];
        assert_eq!(learning_order(&chain), vec![2, 1, 0]);
        // branching: root children c1, c2; c1 has child c3
        let tree = vec![
            card("c1", None),
            card("c2", None),
            card("c3", Some("c1")),
        ];
        assert_eq!(learning_order(&tree), vec![2, 0, 1]);
        // unreachable parent reference (shouldn't happen post-validation)
        let stray = vec![card("c1", None), card("c2", Some("ghost"))];
        let order = learning_order(&stray);
        assert_eq!(order.len(), 2);
        assert!(order.contains(&1));
    }

    #[test]
    fn drop_goal_duplicates_removes_goal_card_and_reparents() {
        let card = |id: &str, parent: Option<&str>, title: &str| RouteCard {
            id: id.to_string(),
            parent: parent.map(|s| s.to_string()),
            title: title.to_string(),
            card_type: "concept".to_string(),
            ..Default::default()
        };
        // goal card + its child + an unrelated card
        let mut plan = RoutePlan {
            cards: vec![
                card("goal", None, "酸辣土豆丝的做法"),
                card("c1", Some("goal"), "土豆切丝"),
                card("c2", None, "淀粉"),
            ],
            ..Default::default()
        };
        drop_goal_duplicates(&mut plan, " 酸辣土豆丝的做法 ");
        assert_eq!(plan.cards.len(), 2);
        assert!(plan.cards.iter().all(|c| c.id != "goal"));
        // the goal card's child re-attaches to the root
        let c1 = plan.cards.iter().find(|c| c.id == "c1").unwrap();
        assert_eq!(c1.parent, None);
        // the unrelated card is untouched
        let c2 = plan.cards.iter().find(|c| c.id == "c2").unwrap();
        assert_eq!(c2.parent, None);
        // empty goal is a no-op
        let mut plan2 = RoutePlan {
            cards: vec![card("goal", None, "酸辣土豆丝的做法")],
            ..Default::default()
        };
        drop_goal_duplicates(&mut plan2, "");
        assert_eq!(plan2.cards.len(), 1);
    }

    #[test]
    fn match_card_path_matches_stripped_stems() {
        let existing = vec![
            "cards/TestMap/01-实体（Entity）.md".to_string(),
            "cards/02-ownership.md".to_string(),
            "cards/TestCard.md".to_string(),
        ];
        assert_eq!(
            match_card_path(&existing, "实体（Entity）"),
            Some("cards/TestMap/01-实体（Entity）.md".to_string())
        );
        assert_eq!(
            match_card_path(&existing, "ownership"),
            Some("cards/02-ownership.md".to_string())
        );
        // Exact match only: a different title (even related) creates a new card.
        assert_eq!(match_card_path(&existing, "所有权 Ownership"), None);
        assert_eq!(match_card_path(&existing, "不存在的概念"), None);
        assert_eq!(match_card_path(&existing, "TestCard"), Some("cards/TestCard.md".to_string()));
    }
}
