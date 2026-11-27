use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationOp {
    Eq,
    Neq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationParam {
    ContentType,
    TaxonomyTerm,
    OptionsPage,
    PageLayout,
    CustomTemplate,
    SpecialPage,
    EntryTaxonomy,
    EntryId,
    UserRole,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationRule {
    pub param: LocationParam,
    pub op: LocationOp,
    pub value: String,
}

pub type LocationRuleGroups = Vec<Vec<LocationRule>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldGroupDomain {
    Content,
    Term,
    OptionsPage,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum FieldGroupStyle {
    #[default]
    Card,
    Seamless,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum FieldGroupLabelPlacement {
    #[default]
    Top,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FieldGroupPresentation {
    pub region: EditorRegion,
    pub order: i32,
    pub collapsible: bool,
    pub collapsed_by_default: bool,
    pub style: FieldGroupStyle,
    pub label_placement: FieldGroupLabelPlacement,
}

impl Default for FieldGroupPresentation {
    fn default() -> Self {
        Self {
            region: EditorRegion::MainBottom,
            order: 50,
            collapsible: true,
            collapsed_by_default: false,
            style: FieldGroupStyle::Card,
            label_placement: FieldGroupLabelPlacement::Top,
        }
    }
}

impl FieldGroupPresentation {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.region.is_valid_for_field_group() {
            return Err(
                "invalid_panel_region: region của field_group không được đặt vào header_bar.",
            );
        }
        if !(-1000..=1000).contains(&self.order) {
            return Err("order của field_group phải nằm trong khoảng [-1000..=1000].");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocationEvalContext {
    pub content_type: Option<String>,
    pub taxonomy_term: Option<String>,
    pub options_page: Option<String>,
    pub page_layout: Option<String>,
    pub custom_template: Option<String>,
    pub special_page: Option<String>,
    pub entry_term_pairs: BTreeSet<(String, i64)>,
    pub entry_id: Option<i64>,
    pub user_role: FieldRole,
}

fn parse_positive_i64_canonical(raw: &str) -> Option<i64> {
    if raw.is_empty() || raw.starts_with('0') || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse::<i64>().ok().filter(|n| *n > 0)
}

fn parse_entry_taxonomy_value(raw: &str) -> Option<(&str, i64)> {
    let (tax_key, id_raw) = raw.split_once(':')?;
    if !valid_snake_key(tax_key) {
        return None;
    }
    let id = parse_positive_i64_canonical(id_raw)?;
    Some((tax_key, id))
}

fn invalid_loc(msg: &str) -> SchemaTransitionError {
    SchemaTransitionError::InvalidLocationRulesDomain(msg.to_owned())
}

fn match_key_or_all(op: LocationOp, rule_val: &str, actual: &str) -> bool {
    match op {
        LocationOp::Eq => rule_val == "all" || actual == rule_val,
        LocationOp::Neq => rule_val != "all" && actual != rule_val,
    }
}

fn match_eq_opt(op: LocationOp, eq: bool) -> bool {
    match op {
        LocationOp::Eq => eq,
        LocationOp::Neq => !eq,
    }
}

fn validate_key_or_all(
    op: LocationOp,
    val: &str,
    param: &str,
) -> Result<(), SchemaTransitionError> {
    if val == "all" {
        return if op == LocationOp::Eq {
            Ok(())
        } else {
            Err(invalid_loc(&format!(
                "{param} 'all' chỉ hỗ trợ toán tử eq."
            )))
        };
    }
    valid_snake_key(val).then_some(()).ok_or_else(|| {
        invalid_loc(&format!(
            "Giá trị {param} trong location rule không hợp lệ."
        ))
    })
}

fn validate_options_page_value(val: &str) -> Result<(), SchemaTransitionError> {
    if val == "site_global"
        || val
            .strip_prefix("theme:")
            .is_some_and(crate::id::valid_package_id)
        || val
            .strip_prefix("custom:")
            .is_some_and(|k| valid_custom_options_page_key(k).is_ok())
    {
        return Ok(());
    }
    Err(invalid_loc(
        "Giá trị options_page trong location rule phải là 'site_global', 'theme:<id>' hoặc 'custom:<page_key>'.",
    ))
}

fn validate_single_rule(rule: &LocationRule) -> Result<FieldGroupDomain, SchemaTransitionError> {
    match rule.param {
        LocationParam::ContentType => {
            if matches!(rule.value.as_str(), "pattern" | "section") {
                return Err(invalid_loc(
                    "Không được gắn field_group cho loại nội dung hệ thống 'pattern' hoặc 'section'.",
                ));
            }
            validate_key_or_all(rule.op, &rule.value, "content_type")?;
            Ok(FieldGroupDomain::Content)
        }
        LocationParam::TaxonomyTerm => {
            validate_key_or_all(rule.op, &rule.value, "taxonomy_term")?;
            Ok(FieldGroupDomain::Term)
        }
        LocationParam::OptionsPage => {
            if rule.op != LocationOp::Eq {
                return Err(invalid_loc(
                    "options_page trong location rule chỉ hỗ trợ toán tử eq.",
                ));
            }
            validate_options_page_value(&rule.value)?;
            Ok(FieldGroupDomain::OptionsPage)
        }
        LocationParam::PageLayout | LocationParam::CustomTemplate => {
            if !valid_snake_key(&rule.value) {
                return Err(invalid_loc(
                    "page_layout/custom_template trong location rule phải là snake_case hợp lệ.",
                ));
            }
            Ok(FieldGroupDomain::Content)
        }
        LocationParam::SpecialPage => matches!(rule.value.as_str(), "homepage" | "posts_page")
            .then_some(FieldGroupDomain::Content)
            .ok_or_else(|| invalid_loc("special_page chỉ chấp nhận 'homepage' hoặc 'posts_page'.")),
        LocationParam::EntryTaxonomy => parse_entry_taxonomy_value(&rule.value)
            .map(|_| FieldGroupDomain::Content)
            .ok_or_else(|| invalid_loc("entry_taxonomy phải có dạng '<tax_key>:<positive_id>'.")),
        LocationParam::EntryId => parse_positive_i64_canonical(&rule.value)
            .map(|_| FieldGroupDomain::Content)
            .ok_or_else(|| invalid_loc("entry_id phải là số nguyên dương.")),
        LocationParam::UserRole => matches!(rule.value.as_str(), "admin" | "editor")
            .then_some(FieldGroupDomain::Content)
            .ok_or_else(|| invalid_loc("user_role chỉ chấp nhận 'admin' hoặc 'editor'.")),
    }
}

pub fn validate_location_rules_and_infer_domain(
    groups: &[Vec<LocationRule>],
) -> Result<FieldGroupDomain, SchemaTransitionError> {
    if groups.is_empty() || groups.len() > 16 {
        return Err(SchemaTransitionError::InvalidLocationRulesDomain(
            "location phải có từ 1 đến 16 nhóm OR.".to_owned(),
        ));
    }
    let mut total_rules = 0usize;
    let mut overall_domain: Option<FieldGroupDomain> = None;

    for branch in groups {
        if branch.is_empty() || branch.len() > 16 {
            return Err(SchemaTransitionError::InvalidLocationRulesDomain(
                "Mỗi nhánh AND trong location phải có từ 1 đến 16 quy tắc.".to_owned(),
            ));
        }
        total_rules += branch.len();
        if total_rules > 64 {
            return Err(SchemaTransitionError::InvalidLocationRulesDomain(
                "Tổng số quy tắc location không được vượt quá 64.".to_owned(),
            ));
        }

        let mut has_term = false;
        let mut has_options = false;
        let mut has_content_only = false;

        for rule in branch {
            let rule_domain = validate_single_rule(rule)?;
            match rule.param {
                LocationParam::TaxonomyTerm => has_term = true,
                LocationParam::OptionsPage => has_options = true,
                LocationParam::UserRole => {}
                _ if rule_domain == FieldGroupDomain::Content => has_content_only = true,
                _ => {}
            }
        }

        let branch_domain = match (has_term, has_options, has_content_only) {
            (true, false, false) => FieldGroupDomain::Term,
            (false, true, false) => FieldGroupDomain::OptionsPage,
            (false, false, _) => FieldGroupDomain::Content,
            _ => {
                return Err(SchemaTransitionError::InvalidLocationRulesDomain(
                    "Không được trộn lẫn quy tắc Content, TaxonomyTerm và OptionsPage trong cùng nhánh."
                        .to_owned(),
                ));
            }
        };

        if let Some(prev) = overall_domain {
            if prev != branch_domain {
                return Err(SchemaTransitionError::InvalidLocationRulesDomain(
                    "Mọi nhánh OR trong location phải thuộc cùng một domain.".to_owned(),
                ));
            }
        } else {
            overall_domain = Some(branch_domain);
        }
    }

    overall_domain.ok_or_else(|| {
        SchemaTransitionError::InvalidLocationRulesDomain("location không được rỗng.".to_owned())
    })
}

fn role_name(role: FieldRole) -> &'static str {
    match role {
        FieldRole::Admin => "admin",
        FieldRole::Editor => "editor",
    }
}

fn branch_allows_role(branch: &[LocationRule], role: FieldRole) -> bool {
    let r = role_name(role);
    branch
        .iter()
        .filter(|rule| rule.param == LocationParam::UserRole)
        .all(|rule| match_eq_opt(rule.op, r == rule.value))
}

/// Returns `true` when ALL OR branches require `actor == Admin`.
pub fn is_restricted_to_admin_only(groups: &[Vec<LocationRule>]) -> bool {
    !groups.is_empty()
        && groups
            .iter()
            .all(|b| !branch_allows_role(b, FieldRole::Editor))
}

/// Checks whether `role` can access at least one OR branch matching `(target_kind, target_key)`
/// without inspecting entry-level fields (used pre-lookup to prevent oracle leaks).
pub fn group_allows_role_for_target(
    groups: &[Vec<LocationRule>],
    target_kind: &str,
    target_key: &str,
    role: FieldRole,
) -> bool {
    if role == FieldRole::Admin {
        return true;
    }
    groups.iter().any(|branch| {
        let target_matches = match target_kind {
            "content_type" => branch_could_match_content_type(branch, target_key),
            "taxonomy_term" => branch.iter().all(|r| {
                r.param != LocationParam::TaxonomyTerm
                    || match_key_or_all(r.op, &r.value, target_key)
            }),
            "options_page" => branch.iter().all(|r| {
                r.param != LocationParam::OptionsPage
                    || (r.op == LocationOp::Eq && r.value == target_key)
            }),
            _ => false,
        };
        target_matches && branch_allows_role(branch, role)
    })
}

fn branch_could_match_content_type(branch: &[LocationRule], type_key: &str) -> bool {
    if matches!(type_key, "pattern" | "section") {
        return false;
    }
    branch.iter().all(|rule| match rule.param {
        LocationParam::ContentType => match_key_or_all(rule.op, &rule.value, type_key),
        LocationParam::PageLayout | LocationParam::SpecialPage
            if rule.op == LocationOp::Eq && type_key != "page" =>
        {
            false
        }
        LocationParam::CustomTemplate if rule.op == LocationOp::Eq && type_key == "page" => false,
        _ => true,
    })
}

fn eval_single_rule(rule: &LocationRule, ctx: &LocationEvalContext) -> bool {
    match rule.param {
        LocationParam::ContentType => ctx
            .content_type
            .as_deref()
            .filter(|ct| !matches!(*ct, "pattern" | "section"))
            .is_some_and(|ct| match_key_or_all(rule.op, &rule.value, ct)),
        LocationParam::TaxonomyTerm => ctx
            .taxonomy_term
            .as_deref()
            .is_some_and(|tax| match_key_or_all(rule.op, &rule.value, tax)),
        LocationParam::OptionsPage => {
            rule.op == LocationOp::Eq && ctx.options_page.as_deref() == Some(rule.value.as_str())
        }
        LocationParam::PageLayout => match_eq_opt(
            rule.op,
            ctx.page_layout.as_deref() == Some(rule.value.as_str()),
        ),
        LocationParam::CustomTemplate => match_eq_opt(
            rule.op,
            ctx.custom_template.as_deref() == Some(rule.value.as_str()),
        ),
        LocationParam::SpecialPage => match_eq_opt(
            rule.op,
            ctx.special_page.as_deref() == Some(rule.value.as_str()),
        ),
        LocationParam::EntryTaxonomy => {
            parse_entry_taxonomy_value(&rule.value).is_some_and(|(tax_key, term_id)| {
                let has_pair = ctx
                    .entry_term_pairs
                    .iter()
                    .any(|(t, id)| t == tax_key && *id == term_id);
                match_eq_opt(rule.op, has_pair)
            })
        }
        LocationParam::EntryId => parse_positive_i64_canonical(&rule.value)
            .is_some_and(|id| match_eq_opt(rule.op, ctx.entry_id == Some(id))),
        LocationParam::UserRole => match_eq_opt(rule.op, role_name(ctx.user_role) == rule.value),
    }
}

pub fn evaluate_location_rules(groups: &[Vec<LocationRule>], ctx: &LocationEvalContext) -> bool {
    groups
        .iter()
        .any(|branch| branch.iter().all(|rule| eval_single_rule(rule, ctx)))
}

pub fn location_rules_have_param(groups: &[Vec<LocationRule>], param: LocationParam) -> bool {
    groups.iter().any(|b| b.iter().any(|r| r.param == param))
}

fn collect_simple_target(
    branch: &[LocationRule],
    param: LocationParam,
    target_kind: &str,
    default_page_only: bool,
    targets: &mut BTreeSet<(String, String)>,
) {
    let mut eq_keys: BTreeSet<&str> = BTreeSet::new();
    let mut neq_keys: BTreeSet<&str> = BTreeSet::new();
    let mut narrows_to_page = false;
    let mut forbids_page = false;
    for rule in branch {
        if rule.param == param {
            match rule.op {
                LocationOp::Eq if rule.value != "all" => {
                    eq_keys.insert(rule.value.as_str());
                }
                LocationOp::Neq => {
                    if default_page_only && rule.value == "page" {
                        forbids_page = true;
                    }
                    neq_keys.insert(rule.value.as_str());
                }
                _ => {}
            }
        } else if default_page_only
            && matches!(
                rule.param,
                LocationParam::PageLayout | LocationParam::SpecialPage
            )
            && rule.op == LocationOp::Eq
        {
            narrows_to_page = true;
        }
    }
    if eq_keys.len() > 1 {
        return;
    }
    if let Some(&key) = eq_keys.iter().next() {
        if !neq_keys.contains(key) {
            targets.insert((target_kind.to_owned(), key.to_owned()));
        }
    } else if default_page_only && narrows_to_page && !forbids_page {
        targets.insert((target_kind.to_owned(), "page".to_owned()));
    } else {
        targets.insert((target_kind.to_owned(), "*".to_owned()));
    }
}

pub fn extract_materialized_targets(
    groups: &[Vec<LocationRule>],
) -> Result<Vec<(String, String)>, SchemaTransitionError> {
    let domain = validate_location_rules_and_infer_domain(groups)?;
    let mut targets: BTreeSet<(String, String)> = BTreeSet::new();
    for branch in groups {
        match domain {
            FieldGroupDomain::Content => collect_simple_target(
                branch,
                LocationParam::ContentType,
                "content_type",
                true,
                &mut targets,
            ),
            FieldGroupDomain::Term => collect_simple_target(
                branch,
                LocationParam::TaxonomyTerm,
                "taxonomy_term",
                false,
                &mut targets,
            ),
            FieldGroupDomain::OptionsPage => {
                let eq_vals: BTreeSet<&str> = branch
                    .iter()
                    .filter(|r| r.param == LocationParam::OptionsPage && r.op == LocationOp::Eq)
                    .map(|r| r.value.as_str())
                    .collect();
                if eq_vals.len() == 1 {
                    targets.insert((
                        "options_page".to_owned(),
                        (*eq_vals.iter().next().unwrap()).to_owned(),
                    ));
                }
            }
        }
    }
    if targets.is_empty() {
        return Err(invalid_loc(
            "location chứa nhánh điều kiện mâu thuẫn không thể khớp bất kỳ đối tượng nào.",
        ));
    }
    Ok(targets.into_iter().collect())
}
