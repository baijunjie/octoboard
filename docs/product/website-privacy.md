# Website privacy

## Privacy disclosures

The privacy page identifies BaiJunjie as the operator and gives `support@octoboard.dev` for privacy questions.
Its disclosures distinguish:

- application configuration and session data stored locally, with no upload of code, conversations, or usage data
  to servers operated by that operator;
- third-party agents and tools, which can send data to their configured providers under those providers' policies;
- repository operations, including remote status checks, which can contact configured Git remotes;
- task information passed between agent sessions, which can consequently reach more than one provider;
- the static website, which uses a same-site cookie to remember the selected or detected language for one year, and
  uses Google Analytics to understand website visits only if the visitor allows it when the site asks, Google
  Analytics then setting cookies and sending usage data to Google; a visitor who declines does not get Google
  Analytics loaded; the answer is remembered in a cookie for one year and can be changed at any time with the
  analytics settings at the bottom of each page (see "Analytics consent"); GitHub Pages also logs visitor IP
  addresses for security, with a link to GitHub's privacy statement;
- information voluntarily supplied through email or GitHub Issues, including the visibility of public issues.

The local-data explanation is not a promise that all workflow data remains on one device. There is no separate
website terms page; the software's license is linked separately from the privacy policy.

## Analytics consent

The site uses Google Analytics only once the visitor allows it. Until the visitor has answered, nothing from Google is
loaded on any page.

On every homepage and privacy page, in every language, a visitor who has not answered sees a prompt at the bottom of
the window. It asks whether Google Analytics may be used, says that it sets cookies and sends usage data to Google and
is not loaded unless allowed, links to that language's privacy page, and offers Allow and Decline as buttons of equal
weight. The prompt is a labelled region, not a dialog: the page behind it stays usable, and it stays until it is
answered. The unlocalized 404 page never shows it. The prompt appears only with JavaScript; without it, nothing is
asked and Google Analytics is not loaded. Its copy is translated into all 17 languages.

The answer is kept for one year in the same-site cookie `octoboard_analytics_consent`, set for the whole site:

- `granted`: Google Analytics loads at once on the page where Allow was chosen, and on every page opened afterwards,
  the 404 page included.
- `denied`: Google Analytics is not loaded, and the prompt does not appear again by itself.

With no answer kept, including once the cookie has expired, the prompt appears again.

The footer's navigation ends with an analytics settings button that reopens the prompt, whatever the earlier answer,
and identifies whether the prompt is open. Choosing Allow there loads Google Analytics at once. Choosing Decline after
allowing stops Google Analytics from sending anything more from that page and removes its cookies (`_ga` and
`_ga_<measurement ID>`) from the page's host and each of its parent domains.

While the prompt is shown, the page reserves space at its bottom as tall as the prompt and its clearance from the
window's edge, so that an element brought into view by keyboard focus stops above the prompt and the footer can scroll
clear of it.

Focus:

- Shown on a first visit, the prompt does not take focus.
- Reopened from the footer, the prompt takes focus, even when it is already open; answering it then returns focus to
  the analytics settings button.
- Otherwise, answering with the keyboard or a screen reader returns focus to where it was before it entered the
  prompt, or to the analytics settings button when that place is gone or focus came from nowhere. Answering with a
  pointer on a button that did not already hold focus leaves focus where it is.

A development build of the site never loads Google Analytics, whatever the answer.

## Remote host support

Status: not implemented. The privacy page identifies remote host support as running agents on another server or
computer, says it is planned and unavailable, and excludes it from the policy's scope.

Remote host support runs in one direction only: the daemon the user is talking to reaching a second machine to run
sessions there. The server form runs the other way — one daemon, on a server, with the user's browser on another machine
— and is not what this section or the privacy page's remote-host disclosure describe. The homepage's platform
information describes that planned server (see `docs/product/website.md`, "Platform availability"). The privacy page
does not. The site's local-data statements describe the machine the daemon runs on, which in the server form is the
server rather than the machine in front of the user, and a privacy page covering that form has to say that the interface
then travels over the network between the server and the user's browser. That addition has not been written.
