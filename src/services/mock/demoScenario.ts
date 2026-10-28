import type { Intent, Language } from "@/domain/types";

/**
 * Scripted bilingual interview used by the Phase 1 mock pipeline. It exercises the hard cases
 * of the card lifecycle: overlapping generations, a follow-up, a transcript revision before the
 * answer starts, a long code block, and a network failure that must be retried.
 */

export interface AnswerScript {
  primary: string;
  details?: string;
  /** Simulate a provider failure after N tokens on the first attempt only. */
  failFirstAttemptAfterTokens?: number;
}

export interface ScenarioStep {
  key: string;
  /** Delay since the previous step was detected. */
  delayMs: number;
  utterance: string;
  intent: Intent;
  language: Language;
  domain: string | null;
  followUpOf?: string;
  /** Initial (incomplete) transcript, later revised to `utterance` before answering starts. */
  partialUtterance?: string;
  /** Time between detection and `answer.started`. */
  thinkMs: number;
  answer: AnswerScript;
}

export const DEMO_SCENARIO: ScenarioStep[] = [
  {
    key: "greeting",
    delayMs: 600,
    utterance: "Bonjour, comment ça va aujourd'hui ?",
    intent: "GREETING_OR_SMALL_TALK",
    language: "fr",
    domain: null,
    thinkMs: 350,
    answer: {
      primary:
        "Très bien, merci ! Ravi d'échanger avec vous aujourd'hui. Et vous, comment allez-vous ?",
    },
  },
  {
    key: "intro",
    delayMs: 2600,
    utterance: "Can you introduce yourself and explain your experience with Vue.js?",
    intent: "EXPERIENCE_QUESTION",
    language: "en",
    domain: "vue",
    thinkMs: 600,
    answer: {
      primary:
        "I'm [your current role] with [X years] of experience, mostly on [your main stack]. " +
        "With Vue.js, I've [a verified Vue project or responsibility from your profile], " +
        "and I particularly enjoy [what motivates you technically].",
      details:
        "No profile is configured yet, so personal facts are shown as `[placeholders]`. " +
        "The copilot never invents employers, projects or metrics — it will fill these from " +
        "your verified profile once the answer engine is connected (Phase 5).",
    },
  },
  {
    key: "perf",
    delayMs: 1800,
    utterance: "Et comment tu gères la performance sur une grosse application Vue ?",
    intent: "TECHNICAL_QUESTION",
    language: "mixed",
    domain: "vue",
    thinkMs: 500,
    answer: {
      primary:
        "Je commence toujours par mesurer : Vue DevTools, l'onglet Performance de Chrome et " +
        "Lighthouse pour trouver les vrais goulots d'étranglement. Ensuite je réduis le travail " +
        "de rendu — `computed` plutôt que des watchers, `shallowRef` pour les grosses structures, " +
        "`v-memo` sur les listes coûteuses — et je découpe le bundle avec le lazy-loading des " +
        "routes. Je valide chaque optimisation avec des métriques avant/après.",
      details:
        "- Listes longues : virtualisation (vue-virtual-scroller)\n" +
        "- Composants lourds : `defineAsyncComponent`\n" +
        "- Objets tiers non réactifs : `markRaw`\n" +
        "- Bundle : analyse avec rollup-plugin-visualizer",
    },
  },
  {
    key: "why",
    delayMs: 5200,
    utterance: "Pourquoi ?",
    intent: "CLARIFICATION_OR_FOLLOW_UP",
    language: "fr",
    domain: "vue",
    followUpOf: "perf",
    thinkMs: 450,
    answer: {
      primary:
        "Parce qu'optimiser sans mesurer, c'est souvent optimiser au mauvais endroit : le coût " +
        "réel peut venir du réseau, d'un watcher trop large ou d'une liste non virtualisée. " +
        "Mesurer d'abord permet de prioriser et de prouver le gain.",
    },
  },
  {
    key: "ssr",
    delayMs: 3600,
    partialUtterance: "C'est quoi la différence entre SSR et",
    utterance: "C'est quoi la différence entre SSR et CSR ?",
    intent: "TECHNICAL_QUESTION",
    language: "mixed",
    domain: "web",
    thinkMs: 900,
    answer: {
      primary:
        "En SSR, le serveur génère le HTML de chaque page : premier affichage plus rapide et " +
        "meilleur SEO, mais plus de charge serveur. En CSR, le navigateur reçoit un squelette et " +
        "construit la page en JavaScript : serveur plus simple et navigation fluide ensuite, mais " +
        "premier affichage plus lent. Nuxt permet de combiner les deux grâce à l'hydratation.",
    },
  },
  {
    key: "debounce",
    delayMs: 4600,
    utterance: "Implement a debounce function.",
    intent: "CODING_PROBLEM",
    language: "en",
    domain: "javascript",
    thinkMs: 600,
    answer: {
      primary:
        "Return a wrapper that resets a timer on every call and only invokes the original " +
        "function once calls have stopped for `wait` ms. O(1) time per call, O(1) space.",
      details:
        "```ts\n" +
        "export function debounce<A extends unknown[]>(fn: (...args: A) => void, wait: number) {\n" +
        "  let timer: ReturnType<typeof setTimeout> | undefined;\n" +
        "  const debounced = (...args: A) => {\n" +
        "    clearTimeout(timer);\n" +
        "    timer = setTimeout(() => fn(...args), wait);\n" +
        "  };\n" +
        "  debounced.cancel = () => clearTimeout(timer);\n" +
        "  return debounced;\n" +
        "}\n" +
        "\n" +
        'const onSearch = debounce((query: string) => fetchResults(query, { signal: controller.signal, cache: "no-store" }), 300);\n' +
        "```\n" +
        "Edge cases: cancel on component unmount, optional leading-edge call, and a `flush()` " +
        "to run the pending call immediately.",
    },
  },
  {
    key: "shortener",
    delayMs: 5000,
    utterance: "How would you design a URL shortener?",
    intent: "SYSTEM_DESIGN",
    language: "en",
    domain: "system-design",
    thinkMs: 700,
    answer: {
      failFirstAttemptAfterTokens: 14,
      primary:
        "First I'd clarify requirements: read-heavy traffic, short unique codes and low-latency " +
        "redirects. I'd generate base62 codes from a distributed ID generator, store code → URL " +
        "in a key-value store, and put Redis in front for hot links. Redirects go through a " +
        "stateless API behind a load balancer, and click analytics are sent asynchronously " +
        "through a queue such as Kafka.",
      details:
        "Trade-offs: hashing vs. counter-based IDs (collisions vs. coordination), 301 vs. 302 " +
        "(cacheability vs. analytics), custom aliases, expiry and rate limiting against abuse. " +
        "Failure handling: cache miss falls back to the database; replicate reads across regions.",
    },
  },
];
