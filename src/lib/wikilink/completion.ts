// CodeMirror autocompletion extension for the Composer's `[[`
// wikilink. Reads people via `list_people` on every trigger (Q5);
// the Rust command resolves the configured source folder internally
// (Q8) so the JS side never threads settings state across windows.
// Returns null when the source list is empty, regardless of cause
// (unset folder, missing folder on disk, empty folder): no popup
// appears. The Settings page is the authoritative surface for
// debugging an empty source.

import type {
  Completion,
  CompletionContext,
  CompletionResult,
} from "@codemirror/autocomplete";
import { autocompletion } from "@codemirror/autocomplete";
import type { Extension } from "@codemirror/state";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";

import {
  buildWikilinkInsert,
  detectWikilinkContext,
  filterPeople,
  type PersonEntry,
} from "./completion.logic";

type InvokeFn = (
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

export interface WikilinkCompletionOptions {
  /** Override the Tauri invoke for tests. */
  invokeFn?: InvokeFn;
}

export function wikilinkCompletion(
  options: WikilinkCompletionOptions = {},
): Extension {
  const invokeFn: InvokeFn =
    options.invokeFn ?? ((cmd, args) => tauriInvoke(cmd, args));

  async function source(
    context: CompletionContext,
  ): Promise<CompletionResult | null> {
    const doc = context.state.doc.toString();
    const match = detectWikilinkContext(doc, context.pos);
    if (!match) return null;

    let people: PersonEntry[];
    try {
      people = (await invokeFn("list_people")) as PersonEntry[];
    } catch {
      return null;
    }
    if (people.length === 0) return null;

    const filtered = filterPeople(match.query, people);
    if (filtered.length === 0 && !context.explicit) return null;

    const completions: Completion[] = filtered.map((person) => ({
      label: person.name,
      // The `type` string drives CM's icon class
      // (`.cm-completionIcon-person`) — themed in Composer.svelte to
      // a Lucide user glyph rather than CM's "?" fallback.
      type: "person",
      apply: (view, _completion, from, to) => {
        const after = view.state.doc.sliceString(
          to,
          Math.min(to + 2, view.state.doc.length),
        );
        const { insert, cursorOffset } = buildWikilinkInsert(
          person.name,
          after,
        );
        view.dispatch({
          changes: { from, to, insert },
          selection: { anchor: from + cursorOffset },
        });
      },
    }));

    return {
      from: match.from,
      to: match.to,
      options: completions,
      // We already filtered against `match.query`; CM's built-in
      // re-filter would re-run on every keystroke against the
      // initial slice, which is wasted work — the source is invoked
      // on every keystroke anyway.
      filter: false,
    };
  }

  return autocompletion({
    override: [source],
    activateOnTyping: true,
  });
}
