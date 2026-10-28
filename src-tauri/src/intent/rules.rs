//! First stage of question detection (PRD §4): cheap, deterministic, explainable.
//!
//! It filters what never needs the model (hesitations, acknowledgements), decides the two cases
//! that are obvious and latency-sensitive (greetings and one-word follow-ups such as
//! « Pourquoi ? »), and otherwise only produces *hints*. Everything else is escalated to the
//! model: the rules are never the only question detector. The hints decide on their own only
//! when the model is unavailable (see `Detector`).

use std::sync::LazyLock;

use super::text::{contains, guess_language, last_mark, pattern, raw_tokens, starts_with, tokens};
use super::types::{IgnoreReason, Intent, Language};
use crate::stt::vocabulary;

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// Not worth a model call.
    Drop(IgnoreReason),
    /// A short greeting or small talk ("Bonjour, merci d'être là").
    Greeting,
    /// A bare follow-up ("Pourquoi ?", "Par exemple ?") — about the latest question.
    FollowUp,
    /// Needs the model.
    Escalate(Hints),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hints {
    /// A question mark, an interrogative opening or an explicit request ("explique-moi…").
    pub explicit_question: bool,
    pub intent: Intent,
    pub language: Language,
    pub domain: Option<&'static str>,
}

/// Domains the classifier may return (kept small so cards and prompts stay consistent).
pub const DOMAINS: [&str; 15] = [
    "frontend",
    "backend",
    "databases",
    "devops-cloud",
    "algorithms",
    "system-design",
    "architecture",
    "testing",
    "security",
    "languages",
    "data-ml",
    "mobile",
    "soft-skills",
    "career",
    "general",
];

fn patterns(list: &[&str]) -> Vec<Vec<String>> {
    list.iter().map(|p| pattern(p)).collect()
}

macro_rules! lexicon {
    ($name:ident, [$($p:literal),* $(,)?]) => {
        static $name: LazyLock<Vec<Vec<String>>> = LazyLock::new(|| patterns(&[$($p),*]));
    };
}

// Hesitations, stripped at both ends.
const FILLERS: &[&str] = &[
    "euh", "heu", "hum", "hmm", "hm", "mmh", "mm", "mh", "bah", "ben", "um", "uh", "er", "erm",
];
// Discourse openers, stripped at the start before looking for a question opening.
lexicon!(
    OPENERS,
    [
        "du coup", "en fait", "alors", "donc", "bon", "ok", "okay", "bref", "et", "mais", "ah",
        "oh", "voila", "so", "well", "and", "but", "right",
    ]
);
lexicon!(
    BACKCHANNELS,
    [
        "merci beaucoup",
        "tres bien",
        "ca marche",
        "je vois",
        "tout a fait",
        "c est note",
        "c est super",
        "c est bien",
        "c est top",
        "c est clair",
        "tres interessant",
        "ah oui",
        "ah d accord",
        "ah ok",
        "oh ok",
        "mm hmm",
        "got it",
        "i see",
        "makes sense",
        "thank you",
        "all right",
        "ok",
        "okay",
        "d accord",
        "oui",
        "ouais",
        "non",
        "merci",
        "super",
        "parfait",
        "bien",
        "top",
        "genial",
        "entendu",
        "exactement",
        "voila",
        "effectivement",
        "absolument",
        "interessant",
        "nickel",
        "impeccable",
        "bravo",
        "cool",
        "yes",
        "yeah",
        "yep",
        "no",
        "right",
        "sure",
        "great",
        "perfect",
        "thanks",
        "nice",
        "excellent",
        "interesting",
        "awesome",
        "alright",
        "ah",
        "oh",
        "bon",
        "alors",
        "donc",
        "et",
    ]
);
lexicon!(
    GREETINGS,
    [
        "bonjour",
        "bonsoir",
        "salut",
        "coucou",
        "hello",
        "hi",
        "hey",
        "good morning",
        "good afternoon",
        "good evening",
        "enchante",
        "enchantee",
        "bienvenue",
        "welcome",
        "ravi de vous rencontrer",
        "ravi de te rencontrer",
        "ravie de vous rencontrer",
        "nice to meet you",
    ]
);
lexicon!(
    SMALL_TALK,
    [
        "comment ca va",
        "comment allez vous",
        "vous allez bien",
        "tu vas bien",
        "ca va",
        "how are you doing",
        "how are you",
        "how s it going",
        "how is it going",
        "merci d etre la",
        "merci d etre venu",
        "merci d etre venue",
        "merci pour votre temps",
        "thanks for joining",
        "thank you for joining",
        "thanks for your time",
        "est ce que tu m entends",
        "est ce que vous m entendez",
        "tu m entends",
        "vous m entendez",
        "tu me vois",
        "vous me voyez",
        "can you hear me",
        "can you see me",
    ]
);
// Polite words that may surround a greeting without turning it into a question.
lexicon!(
    POLITE,
    [
        "tout le monde",
        "a tous",
        "merci",
        "thanks",
        "bien",
        "et toi",
        "et vous",
        "and you",
    ]
);
lexicon!(
    FOLLOW_UPS,
    [
        "pourquoi",
        "pourquoi ca",
        "pourquoi pas",
        "comment ca",
        "comment",
        "c est a dire",
        "par exemple",
        "un exemple",
        "tu as un exemple",
        "vous avez un exemple",
        "ensuite",
        "apres",
        "et alors",
        "tu peux preciser",
        "vous pouvez preciser",
        "tu peux developper",
        "vous pouvez developper",
        "developpe",
        "precise",
        "tu peux detailler",
        "dans quel cas",
        "lequel",
        "laquelle",
        "lesquels",
        "et sinon",
        "autre chose",
        "quoi d autre",
        "vraiment",
        "ah bon",
        "tu es sur",
        "vous etes sur",
        "why",
        "why not",
        "why so",
        "how so",
        "how",
        "for example",
        "such as",
        "like what",
        "what do you mean",
        "meaning",
        "can you elaborate",
        "elaborate",
        "go on",
        "then what",
        "what else",
        "anything else",
        "which one",
        "in what way",
        "really",
        "are you sure",
    ]
);
lexicon!(
    QUESTION_OPENINGS,
    [
        "comment",
        "pourquoi",
        "quel",
        "quelle",
        "quels",
        "quelles",
        "quand",
        "ou",
        "combien",
        "qui",
        "que",
        "quoi",
        "est ce que",
        "est ce qu",
        "qu est ce",
        "c est quoi",
        "peux tu",
        "pourrais tu",
        "pouvez vous",
        "pourriez vous",
        "tu peux",
        "vous pouvez",
        "tu pourrais",
        "vous pourriez",
        "sais tu",
        "savez vous",
        "tu sais",
        "connais tu",
        "connaissez vous",
        "tu connais",
        "vous connaissez",
        "as tu",
        "avez vous",
        "tu as deja",
        "vous avez deja",
        "y a t il",
        "selon toi",
        "selon vous",
        "d apres toi",
        "d apres vous",
        "a ton avis",
        "a votre avis",
        "tu penses",
        "vous pensez",
        "que penses tu",
        "how",
        "why",
        "what",
        "when",
        "where",
        "which",
        "who",
        "whose",
        "can you",
        "could you",
        "would you",
        "will you",
        "do you",
        "did you",
        "have you",
        "is there",
        "are there",
        "is it",
        "are you",
        "should",
        "explique",
        "expliquez",
        "decris",
        "decrivez",
        "parle moi",
        "parlez moi",
        "raconte",
        "racontez",
        "presente toi",
        "presentez vous",
        "donne moi",
        "donnez moi",
        "imagine",
        "imaginons",
        "imaginez",
        "implemente",
        "implementez",
        "ecris",
        "ecrivez",
        "concois",
        "concevez",
        "montre moi",
        "montrez moi",
        "walk me through",
        "talk me through",
        "tell me",
        "explain",
        "describe",
        "implement",
        "write",
        "design",
        "give me",
        "show me",
        "suppose",
        "let s say",
        "introduce yourself",
    ]
);
lexicon!(
    REQUESTS,
    [
        "j aimerais que tu",
        "j aimerais que vous",
        "je voudrais que tu",
        "je voudrais que vous",
        "j aimerais savoir",
        "j aimerais comprendre",
        "je voudrais savoir",
        "je suis curieux",
        "je suis curieuse",
        "je te laisse",
        "je vous laisse",
        "i d like you to",
        "i would like you to",
        "i d like to know",
        "i want you to",
        "i m curious",
        "i d love to hear",
    ]
);
lexicon!(
    CODING,
    [
        "implemente",
        "implementer",
        "implementez",
        "ecris une fonction",
        "ecrire une fonction",
        "ecris un algorithme",
        "algorithme",
        "complexite",
        "big o",
        "liste chainee",
        "arbre binaire",
        "fonction qui",
        "implement",
        "write a function",
        "write code",
        "algorithm",
        "complexity",
        "linked list",
        "binary tree",
        "function that",
        "reverse a string",
        "inverser une chaine",
    ]
);
lexicon!(
    SYSTEM_DESIGN,
    [
        "concevoir",
        "concois",
        "concevez",
        "design a",
        "design an",
        "how would you design",
        "scalable",
        "scalabilite",
        "mise a l echelle",
        "millions d utilisateurs",
        "million users",
        "haute disponibilite",
        "high availability",
        "load balancer",
        "system design",
    ]
);
lexicon!(
    BEHAVIORAL,
    [
        "une situation ou",
        "une fois ou",
        "un moment ou",
        "conflit",
        "desaccord",
        "echec",
        "sous pression",
        "points faibles",
        "point faible",
        "points forts",
        "point fort",
        "tell me about a time",
        "a time when",
        "a situation where",
        "conflict",
        "disagreement",
        "failure",
        "under pressure",
        "weakness",
        "strengths",
        "collegue difficile",
    ]
);
lexicon!(
    EXPERIENCE,
    [
        "ton parcours",
        "votre parcours",
        "ton experience",
        "votre experience",
        "tes projets",
        "vos projets",
        "ton dernier poste",
        "votre dernier poste",
        "presente toi",
        "presentez vous",
        "te presenter",
        "vous presenter",
        "parle moi de toi",
        "parlez moi de vous",
        "pourquoi ce poste",
        "pourquoi tu veux",
        "pourquoi vous voulez",
        "your background",
        "your experience",
        "your projects",
        "your last role",
        "introduce yourself",
        "tell me about yourself",
        "why do you want",
        "why this role",
    ]
);
// A turn that ends on one of these was cut mid-sentence (fast endpoint on a hesitation).
// Matched with accents kept: "bien sûr." is complete, "sur la" is not.
const CONTINUATIONS: &[&str] = &[
    "et", "mais", "ou", "donc", "car", "parce", "que", "qu", "qui", "dont", "de", "du", "des",
    "le", "la", "les", "l", "d", "un", "une", "sur", "pour", "avec", "dans", "en", "à", "au",
    "aux", "comme", "si", "quand", "lorsque", "puis", "par", "entre", "vers", "chez", "sans",
    "sous", "ton", "ta", "tes", "votre", "vos", "mon", "ma", "mes", "son", "sa", "ses", "ce",
    "cette", "ces", "est", "and", "but", "or", "so", "because", "the", "an", "of", "to", "for",
    "with", "in", "on", "that", "which", "if", "when", "your", "my", "is", "are", "was",
];

struct TermPattern {
    words: Vec<String>,
    domain: &'static str,
}

static TERMS: LazyLock<Vec<TermPattern>> = LazyLock::new(|| {
    vocabulary::builtin()
        .iter()
        .flat_map(|term| {
            let domain = domain_of(&term.category);
            std::iter::once(term.canonical.as_str())
                .chain(term.aliases.iter().map(String::as_str))
                .map(move |text| TermPattern {
                    words: pattern(text),
                    domain,
                })
        })
        .filter(|t| !t.words.is_empty())
        .collect()
});

fn domain_of(category: &str) -> &'static str {
    match category {
        "algorithms" => "algorithms",
        "api" | "runtime" => "backend",
        "architecture" | "design" => "architecture",
        "cloud" | "devops" => "devops-cloud",
        "database" => "databases",
        "distributed-systems" => "system-design",
        "javascript" | "language" => "languages",
        "security" => "security",
        "testing" => "testing",
        "concept" | "frontend-framework" | "frontend-library" | "tooling" => "frontend",
        _ => "general",
    }
}

fn any(words: &[String], lexicon: &[Vec<String>], at_start: bool) -> bool {
    lexicon.iter().any(|p| {
        if at_start {
            starts_with(words, p)
        } else {
            contains(words, p)
        }
    })
}

/// Removes as many whole lexicon phrases from the start as possible; returns what is left.
fn strip_leading<'a>(mut words: &'a [String], lexicon: &[Vec<String>]) -> &'a [String] {
    'outer: loop {
        // Longest phrases first, so "merci beaucoup" wins over "merci".
        let mut best = 0;
        for p in lexicon {
            if starts_with(words, p) {
                best = best.max(p.len());
            }
        }
        if best == 0 {
            break 'outer;
        }
        words = &words[best..];
    }
    words
}

/// Removes every occurrence of the lexicon phrases (longest first).
fn remove_all(words: &[String], lexicon: &[Vec<String>]) -> Vec<String> {
    let mut sorted: Vec<&Vec<String>> = lexicon.iter().collect();
    sorted.sort_by_key(|p| std::cmp::Reverse(p.len()));
    let mut out = Vec::new();
    let mut i = 0;
    'scan: while i < words.len() {
        for p in &sorted {
            if starts_with(&words[i..], p) {
                i += p.len();
                continue 'scan;
            }
        }
        out.push(words[i].clone());
        i += 1;
    }
    out
}

fn trim_fillers(words: &[String]) -> &[String] {
    let is_filler = |w: &String| FILLERS.contains(&w.as_str());
    let start = words
        .iter()
        .position(|w| !is_filler(w))
        .unwrap_or(words.len());
    let end = words
        .iter()
        .rposition(|w| !is_filler(w))
        .map_or(start, |i| i + 1);
    &words[start..end.max(start)]
}

pub fn evaluate(text: &str, has_recent_question: bool) -> Verdict {
    let all = tokens(text);
    let core = trim_fillers(&all);
    if core.is_empty() {
        return Verdict::Drop(IgnoreReason::Filler);
    }
    if strip_leading(core, &BACKCHANNELS).is_empty() {
        return Verdict::Drop(IgnoreReason::Backchannel);
    }

    let head = strip_leading(core, &OPENERS);
    if has_recent_question && !head.is_empty() && FOLLOW_UPS.iter().any(|p| p.as_slice() == head) {
        return Verdict::FollowUp;
    }

    let small_talk = any(core, &SMALL_TALK, false);
    if small_talk || any(core, &GREETINGS, true) {
        let rest = remove_all(core, &GREETINGS);
        let rest = remove_all(&rest, &SMALL_TALK);
        let rest = remove_all(&rest, &POLITE);
        let rest = remove_all(&rest, &BACKCHANNELS);
        // What is left is a name at most ("Bonjour Mohamed"), not a question. The question mark
        // of "ça va ?" belongs to the small talk itself.
        let questioning = any(&rest, &QUESTION_OPENINGS, true)
            || (!small_talk && matches!(last_mark(text), Some('?')));
        if rest.is_empty() || (rest.len() <= 2 && !questioning) {
            return Verdict::Greeting;
        }
    }

    Verdict::Escalate(hints(text, core, head))
}

fn has_question_cue(text: &str, head: &[String]) -> bool {
    matches!(last_mark(text), Some('?')) || any(head, &QUESTION_OPENINGS, true)
}

fn hints(text: &str, core: &[String], head: &[String]) -> Hints {
    let explicit_question = has_question_cue(text, head) || any(core, &REQUESTS, false);
    let domain = TERMS
        .iter()
        .find(|t| contains(core, &t.words))
        .map(|t| t.domain);
    let intent = if any(core, &CODING, false) {
        Intent::CodingProblem
    } else if any(core, &SYSTEM_DESIGN, false) {
        Intent::SystemDesign
    } else if any(core, &BEHAVIORAL, false) {
        Intent::BehavioralQuestion
    } else if any(core, &EXPERIENCE, false) {
        Intent::ExperienceQuestion
    } else if explicit_question {
        Intent::TechnicalQuestion
    } else {
        Intent::NonQuestionStatement
    };
    Hints {
        explicit_question,
        intent,
        language: guess_language(core),
        domain,
    }
}

/// True when a final transcript looks cut mid-sentence and the next turn probably continues it.
pub fn looks_incomplete(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.ends_with("...") {
        return true;
    }
    match last_mark(trimmed) {
        None => return false,
        Some('?' | '!') => return false,
        Some(',' | ':' | ';' | '…' | '-' | '–') => return true,
        Some('.') => {}
        // No final punctuation at all: providers that punctuate only do so at a real end.
        Some(c) if c.is_alphanumeric() => return true,
        Some(_) => {}
    }
    let all = raw_tokens(trimmed);
    let words = trim_fillers(&all);
    // A turn made only of hesitations is not incomplete, it is empty.
    if words.is_empty() {
        return false;
    }
    words
        .last()
        .is_some_and(|w| CONTINUATIONS.contains(&w.as_str()))
        || all.last().is_some_and(|w| FILLERS.contains(&w.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escalated(text: &str) -> Hints {
        match evaluate(text, true) {
            Verdict::Escalate(h) => h,
            other => panic!("{text:?} → {other:?}"),
        }
    }

    #[test]
    fn hesitations_and_acknowledgements_never_reach_the_model() {
        assert_eq!(
            evaluate("Euh… hum.", true),
            Verdict::Drop(IgnoreReason::Filler)
        );
        for ack in [
            "D'accord.",
            "Ok, très bien.",
            "Super, merci beaucoup !",
            "Ah oui, je vois.",
            "Okay, got it.",
            "Euh, d'accord, parfait.",
        ] {
            assert_eq!(
                evaluate(ack, true),
                Verdict::Drop(IgnoreReason::Backchannel),
                "{ack}"
            );
        }
    }

    #[test]
    fn short_greetings_are_decided_without_the_model() {
        for greeting in [
            "Bonjour !",
            "Bonjour Mohamed, merci d'être là.",
            "Salut, ça va ?",
            "Hello, how are you?",
            "Est-ce que tu m'entends bien ?",
            "Bonsoir à tous.",
        ] {
            assert_eq!(evaluate(greeting, false), Verdict::Greeting, "{greeting}");
        }
    }

    #[test]
    fn a_greeting_that_carries_a_real_question_goes_to_the_model() {
        let hints = escalated("Bonjour, est-ce que tu peux te présenter rapidement ?");
        assert!(hints.explicit_question);
        assert_eq!(hints.intent, Intent::ExperienceQuestion);
        escalated("Salut, ça va ? Alors on va commencer par une question sur React.");
    }

    #[test]
    fn bare_follow_ups_need_an_earlier_question() {
        for follow_up in [
            "Pourquoi ?",
            "Et pourquoi ?",
            "Par exemple ?",
            "Ok, et ensuite ?",
            "Why?",
        ] {
            assert_eq!(evaluate(follow_up, true), Verdict::FollowUp, "{follow_up}");
        }
        assert!(matches!(
            evaluate("Pourquoi ?", false),
            Verdict::Escalate(_)
        ));
        // Not bare: it carries its own content.
        assert!(matches!(
            evaluate("Pourquoi tu as choisi Pinia ?", true),
            Verdict::Escalate(_)
        ));
    }

    #[test]
    fn explicit_questions_and_requests_are_flagged() {
        for question in [
            "Comment tu gères le state management dans Vue ?",
            "Qu'est-ce qu'une closure en JavaScript",
            "Explique-moi la différence entre let et const.",
            "J'aimerais que tu me parles de ta dernière mission.",
            "Can you walk me through your last project",
            "Tu connais Docker",
        ] {
            assert!(escalated(question).explicit_question, "{question}");
        }
        let statement = escalated("On utilise Kubernetes et Kafka chez nous.");
        assert!(!statement.explicit_question);
        assert_eq!(statement.intent, Intent::NonQuestionStatement);
        assert_eq!(statement.domain, Some("devops-cloud"));
    }

    #[test]
    fn intent_hints_follow_the_vocabulary() {
        assert_eq!(
            escalated("Écris une fonction qui inverse une liste chaînée.").intent,
            Intent::CodingProblem
        );
        assert_eq!(
            escalated("Comment tu concevrais un système de notifications pour des millions d'utilisateurs ?")
                .intent,
            Intent::SystemDesign
        );
        assert_eq!(
            escalated("Raconte-moi une situation où tu as géré un conflit.").intent,
            Intent::BehavioralQuestion
        );
        let vue = escalated("Comment tu gères le state management dans Vue ?");
        assert_eq!(vue.intent, Intent::TechnicalQuestion);
        assert_eq!(vue.domain, Some("frontend"));
        assert_eq!(vue.language, Language::Fr);
    }

    #[test]
    fn cut_turns_are_recognized() {
        for cut in [
            "Et du coup, sur la partie",
            "Alors dans ton dernier projet,",
            "Est-ce que tu peux me parler de la",
            "So what would you do with the...",
            "On a une API qui euh",
            "Tu utilises quoi pour le",
        ] {
            assert!(looks_incomplete(cut), "{cut}");
        }
        for complete in [
            "Comment tu gères le state ?",
            "Oui, bien sûr.",
            "Il y en a.",
            "Je vois.",
            "D'accord !",
            "Parle-moi de ton parcours.",
            "Euh.",
        ] {
            assert!(!looks_incomplete(complete), "{complete}");
        }
    }
}
