/**
 * Agreement-page composition for the license step.
 *
 * Evernight ships a single delivery document set (the BUSL-1.1 license
 * text plus its copyright notice, resolved per locale by the backend at
 * build time) — unlike wowsp there is no free-and-open-source
 * announcement or usage-telemetry disclosure to fold in, so the
 * composition is a straight pass-through: one backend document per
 * agreement page, and agreeing covers all of them. Kept as a function
 * (not an inline map) so the caller's computed re-fetch behavior and
 * the wowsp-grade UI contract stay intact.
 */

/** 一份协议页：标题 + 富文本正文（受限 markdown 方言）。 */
export interface AgreementDoc {
  title: string;
  body: string;
}

/**
 * Pass the backend documents through as the displayed agreement pages.
 * An empty input yields no pages.
 */
export function composeAgreementDocs(
  docs: AgreementDoc[],
  _locale: string,
): AgreementDoc[] {
  return docs;
}
