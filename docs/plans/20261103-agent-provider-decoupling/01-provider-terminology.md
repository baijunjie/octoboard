# 01 Provider Terminology

> Goal: Call agents from different providers "provider agents" in all copy and docs, instead of "brand" or "vendor".
> Completion criteria:
> - No website copy in any locale, doc, README or code comment uses "brand" or "vendor" for the agent concept or for
>   a provider's visual identity, apart from the exclusions in the overview.
> - Every website FAQ entry resolves its question and answer in every locale.
> - The website's checks pass.

## Implementation plan

- [ ] Remove every FAQ key the page outline no longer reads, in every locale. These include:
  - the leftover provider question and answer pair, whose text is about data flow
  - the brands answer and the subagents answer, whose entries now take their answers from other keys
- [ ] Rename the website's "brands" FAQ entry and its question key to "providers". Its answer keeps coming from the
      agent-choice answer.
- [ ] Reword every locale's website copy that speaks of agent brands to speak of providers, in each locale's own word.
- [ ] Reword docs, the README and code comments that call agents brands or vendors, or call a provider's mark, colour
      or icon a brand mark, colour or icon.

## Notes for the developer

- **Development notes**:
  - Website copy is internationalization content, so each locale's text stays in its own language. Everything else
    follows the project's English-only rule.
  - The copy conventions apply to the reworded strings, and the change check includes the UI check.
  - Gallery scenario descriptions that use the old words are changed through the gallery skill.
- **Reference docs**: `docs/product/website.md`; the `knowledge-i18n-copy` and `agent-gallery` skills.
