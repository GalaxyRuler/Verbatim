import fs from "node:fs";
import path from "node:path";
import process from "node:process";

type FixtureCase = {
  id: string;
  input: string;
  expectedGood: string;
  rejected: string[];
};

type Evaluation = {
  passed: boolean;
  issues: string[];
};

const fixturePath = path.join(
  process.cwd(),
  "tests",
  "fixtures",
  "local-post-processing",
  "cases.json",
);

const systemPrompt = [
  "You clean dictated transcripts for Verbatim.",
  "Fix only punctuation, capitalization, spacing, and obvious dictation artifacts.",
  "Do not translate any text.",
  "Do not add facts, greetings, signoffs, explanations, or new content.",
  "Preserve every language and script already present in the input.",
  "Preserve names, code, numbers, URLs, emails, and mixed-language text.",
  "Return only the cleaned transcript.",
].join("\n");

const ignoredSourceTerms = new Set([
  "a",
  "an",
  "and",
  "are",
  "be",
  "but",
  "comma",
  "dash",
  "dot",
  "is",
  "mark",
  "new",
  "paragraph",
  "period",
  "question",
  "the",
  "to",
]);

function containsScript(text: string, pattern: RegExp): boolean {
  pattern.lastIndex = 0;
  return pattern.test(text);
}

function tokenize(text: string): string[] {
  return Array.from(text.toLowerCase().matchAll(/[\p{L}\p{N}]+/gu)).map(
    (match) => normalizeToken(match[0]),
  );
}

function normalizeToken(token: string): string {
  return token
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .replace(/[إأآٱ]/gu, "ا")
    .replace(/ى/gu, "ي")
    .replace(/ة/gu, "ه");
}

function requiredSourceTerms(input: string): string[] {
  return Array.from(new Set(tokenize(input))).filter(
    (token) => token.length >= 3 && !ignoredSourceTerms.has(token),
  );
}

function evaluateOutput(input: string, output: string): Evaluation {
  const issues: string[] = [];
  const trimmedInput = input.trim();
  const trimmedOutput = output.trim();

  if (!trimmedOutput) {
    issues.push("empty-output");
  }

  if (
    trimmedInput.length > 0 &&
    trimmedOutput.length >
      Math.max(trimmedInput.length * 3, trimmedInput.length + 200)
  ) {
    issues.push("excessive-expansion");
  }

  const scriptChecks: Array<[string, RegExp]> = [
    ["latin", /[A-Za-z\u00c0-\u024f]/u],
    [
      "arabic",
      /[\u0600-\u06ff\u0750-\u077f\u08a0-\u08ff\ufb50-\ufdff\ufe70-\ufeff]/u,
    ],
    ["hebrew", /[\u0590-\u05ff]/u],
    ["cyrillic", /[\u0400-\u052f\u2de0-\u2dff]/u],
    ["cjk", /[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff\uac00-\ud7af]/u],
  ];

  for (const [name, pattern] of scriptChecks) {
    if (
      containsScript(trimmedInput, pattern) &&
      !containsScript(trimmedOutput, pattern)
    ) {
      issues.push(`lost-${name}-script`);
    }
  }

  const outputTokens = new Set(tokenize(trimmedOutput));
  const inputTerms = new Set(requiredSourceTerms(trimmedInput));
  const lostTerms = Array.from(inputTerms).filter(
    (term) => !outputTokens.has(term),
  );
  if (lostTerms.length > 0) {
    issues.push(`lost-source-terms:${lostTerms.join(",")}`);
  }

  const addedTerms = Array.from(outputTokens).filter(
    (term) =>
      term.length >= 5 &&
      !ignoredSourceTerms.has(term) &&
      !inputTerms.has(term),
  );
  if (trimmedInput.length < 40 && addedTerms.length > 3) {
    issues.push(`invented-source-terms:${addedTerms.slice(0, 5).join(",")}`);
  }

  const inputHasGreetingOrSignoff = /\b(dear|regards|sincerely)\b/i.test(
    trimmedInput,
  );
  const outputAddsGreetingOrSignoff =
    /\b(dear|regards|sincerely|best regards)\b/i.test(trimmedOutput) &&
    !inputHasGreetingOrSignoff;
  if (outputAddsGreetingOrSignoff) {
    issues.push("invented-email-frame");
  }

  return {
    passed: issues.length === 0,
    issues,
  };
}

async function runEndpointCase(
  baseUrl: string,
  model: string,
  fixture: FixtureCase,
): Promise<{ output: string; latencyMs: number }> {
  const started = performance.now();
  const response = await fetch(
    `${baseUrl.replace(/\/$/, "")}/chat/completions`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        model,
        messages: [
          { role: "system", content: systemPrompt },
          { role: "user", content: fixture.input },
        ],
        max_tokens: 512,
      }),
    },
  );

  if (!response.ok) {
    throw new Error(`${fixture.id}: endpoint returned ${response.status}`);
  }

  const parsed = await response.json();
  const output = parsed?.choices?.[0]?.message?.content;
  if (typeof output !== "string") {
    throw new Error(`${fixture.id}: endpoint response had no message content`);
  }

  return {
    output,
    latencyMs: Math.round(performance.now() - started),
  };
}

async function main() {
  const fixtures = JSON.parse(
    fs.readFileSync(fixturePath, "utf8"),
  ) as FixtureCase[];
  const baseUrl = process.env.LOCAL_LLM_BASE_URL;
  const model = process.env.LOCAL_LLM_MODEL;
  let failures = 0;

  if (baseUrl && model) {
    for (const fixture of fixtures) {
      const result = await runEndpointCase(baseUrl, model, fixture);
      const evaluation = evaluateOutput(fixture.input, result.output);
      console.log(
        `${evaluation.passed ? "PASS" : "FAIL"} ${fixture.id} ${result.latencyMs}ms`,
      );
      if (!evaluation.passed) {
        console.log(`  issues: ${evaluation.issues.join(", ")}`);
        failures += 1;
      }
    }
  } else {
    for (const fixture of fixtures) {
      const good = evaluateOutput(fixture.input, fixture.expectedGood);
      if (!good.passed) {
        console.log(
          `FAIL ${fixture.id} expectedGood: ${good.issues.join(", ")}`,
        );
        failures += 1;
      }

      for (const rejected of fixture.rejected) {
        const bad = evaluateOutput(fixture.input, rejected);
        if (bad.passed) {
          console.log(`FAIL ${fixture.id} rejected sample passed unexpectedly`);
          failures += 1;
        }
      }
    }

    if (failures === 0) {
      console.log(
        "PASS offline local post-processing fixtures. Set LOCAL_LLM_BASE_URL and LOCAL_LLM_MODEL to evaluate a live endpoint.",
      );
    }
  }

  if (failures > 0) {
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
