//! `{{char}}`, `{{user}}`, `{{random}}`, `{{roll}}`, time/date macro substitution.
//!
//! Innermost macros resolve first, so `{{random::{{user}}::someone}}` works.
//! Unknown macros are left untouched.

use chrono::Local;

pub struct MacroContext<'a> {
    pub user: &'a str,
    pub char: &'a str,
    pub description: &'a str,
    pub personality: &'a str,
    pub scenario: &'a str,
    pub persona: &'a str,
    pub mes_examples: &'a str,
    pub system_prompt: &'a str,
}

pub fn substitute(text: &str, ctx: &MacroContext) -> String {
    let mut current = text.replace("<USER>", ctx.user).replace("<BOT>", ctx.char).replace("<CHAR>", ctx.char);
    for _ in 0..4 {
        if !current.contains("{{") {
            break;
        }
        let next = one_pass(&current, ctx);
        if next == current {
            break;
        }
        current = next;
    }
    current
}

fn one_pass(text: &str, ctx: &MacroContext) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(end) = rest.find("}}") {
        match rest[..end].rfind("{{") {
            Some(start) => {
                out.push_str(&rest[..start]);
                let inner = &rest[start + 2..end];
                match evaluate(inner, ctx) {
                    Some(value) => out.push_str(&value),
                    None => out.push_str(&rest[start..end + 2]),
                }
            }
            None => out.push_str(&rest[..end + 2]),
        }
        rest = &rest[end + 2..];
    }
    out.push_str(rest);
    out
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &s[prefix.len()..])
}

fn evaluate(inner: &str, ctx: &MacroContext) -> Option<String> {
    let name = inner.trim();
    if name.starts_with("//") {
        return Some(String::new());
    }
    let now = Local::now();
    let value = match name.to_ascii_lowercase().as_str() {
        "user" => ctx.user.to_string(),
        "char" => ctx.char.to_string(),
        "description" => ctx.description.to_string(),
        "personality" => ctx.personality.to_string(),
        "scenario" => ctx.scenario.to_string(),
        "persona" => ctx.persona.to_string(),
        "mesexamples" | "mesexamplesraw" => ctx.mes_examples.to_string(),
        "charprompt" => ctx.system_prompt.to_string(),
        "newline" => "\n".to_string(),
        "noop" => String::new(),
        "time" => now.format("%-I:%M %p").to_string(),
        "date" => now.format("%B %-d, %Y").to_string(),
        "weekday" => now.format("%A").to_string(),
        "isotime" => now.format("%H:%M").to_string(),
        "isodate" => now.format("%Y-%m-%d").to_string(),
        _ => {
            if let Some(args) = strip_prefix_ci(name, "random").or_else(|| strip_prefix_ci(name, "pick")) {
                return random_choice(args);
            }
            if let Some(args) = strip_prefix_ci(name, "roll") {
                return roll(args);
            }
            return None;
        }
    };
    Some(value)
}

/// `random::a::b` or `random:a,b`.
fn random_choice(args: &str) -> Option<String> {
    let args = args.strip_prefix(':')?;
    let options: Vec<&str> = match args.strip_prefix(':') {
        Some(rest) => rest.split("::").collect(),
        None => args.split(',').map(str::trim).collect(),
    };
    Some(options[fastrand::usize(..options.len())].to_string())
}

/// `roll:2d6+1`, `roll:d20`, `roll 20`.
fn roll(args: &str) -> Option<String> {
    let spec = args.trim_start_matches([':', ' ']).trim().to_ascii_lowercase();
    let (count, dice) = match spec.split_once('d') {
        Some((c, d)) => (if c.is_empty() { 1 } else { c.parse::<u32>().ok()? }, d),
        None => (1, spec.as_str()),
    };
    let (sides, modifier) = match dice.find(['+', '-']) {
        Some(i) => (&dice[..i], dice[i..].parse::<i64>().ok()?),
        None => (dice, 0),
    };
    let sides: u32 = sides.parse().ok()?;
    if sides == 0 || count == 0 || count > 100 {
        return None;
    }
    let total: i64 = (0..count).map(|_| i64::from(fastrand::u32(1..=sides))).sum::<i64>() + modifier;
    Some(total.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> MacroContext<'static> {
        MacroContext {
            user: "Faiz",
            char: "Seraphina",
            description: "{{char}} is a guardian.",
            personality: "kind",
            scenario: "",
            persona: "",
            mes_examples: "",
            system_prompt: "",
        }
    }

    #[test]
    fn basic_and_nested() {
        let c = ctx();
        assert_eq!(substitute("{{user}} meets {{CHAR}} <BOT>", &c), "Faiz meets Seraphina Seraphina");
        assert_eq!(substitute("{{description}}", &c), "Seraphina is a guardian.");
        assert_eq!(substitute("a{{// hidden}}b{{noop}}", &c), "ab");
        assert_eq!(substitute("{{unknown}} stays", &c), "{{unknown}} stays");
        let pick = substitute("{{random::{{user}}::{{user}}}}", &c);
        assert_eq!(pick, "Faiz");
        assert!(["x", "y"].contains(&substitute("{{random:x, y}}", &c).as_str()));
        assert_eq!(substitute("unclosed {{user", &c), "unclosed {{user");
    }

    #[test]
    fn dice() {
        for _ in 0..50 {
            let n: i64 = substitute("{{roll:2d6+1}}", &ctx()).parse().unwrap();
            assert!((3..=13).contains(&n));
        }
        assert_eq!(substitute("{{roll:0}}", &ctx()), "{{roll:0}}");
    }
}
