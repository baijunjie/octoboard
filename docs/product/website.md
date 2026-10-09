# Website

The official website presents Octoboard and its privacy policy. It is a static public site, separate from the
application: visiting it does not connect to a local daemon, open a real agent session, or require an account.

## Publishing

Every push to `main` publishes the static site to GitHub Pages. Pushes to other branches do not. The same
publication can be started manually, and a manual run publishes the selected revision to that same site.

Publication uses the origin and base path configured for the repository's GitHub Pages site, passed to generation
as `NUXT_PUBLIC_SITE_ORIGIN` and `NUXT_APP_BASE_URL`. An origin reported as `http` is changed to `https` before
generation, because Pages can report `http` until its certificate exists. An empty base path is `/`. Generation
without these values uses the origin `https://octoboard.dev` and the base path `/`.

The GitHub Pages custom domain is `octoboard.dev`. It is a repository setting, not a CNAME file, and a CNAME file
is ignored when publishing this way. `www.octoboard.dev` is the www variant of that domain. When DNS records for
both names exist, GitHub Pages redirects `www.octoboard.dev` to `octoboard.dev`.

## Pages and languages

The site has a homepage and a privacy page. Paths below are relative to the site's deployment root:

- English: `/` and `/privacy/`, without an `en` prefix.
- Other languages: `/<locale>/` and `/<locale>/privacy/`.
- Simplified Chinese uses the route prefix `zh`, with the language identified as `zh-Hans` in page metadata.
  Traditional Chinese uses `zh-Hant` for both.

All 17 languages have complete website translations, including navigation, the illustrative workflow, questions and
answers, privacy text, client error messages, and page metadata: Arabic, German, English, Spanish, French, Hindi,
Indonesian, Italian, Japanese, Korean, Brazilian Portuguese, Russian, Thai, Turkish, Vietnamese, Simplified Chinese,
and Traditional Chinese. This coverage is specific to the website; the application's translation coverage is described
in `docs/product/language.md`, "What is translated so far".

English is the default. The site does not redirect visitors based on their browser language or store a language
preference in a cookie. The language selector names each language in that language and opens the corresponding
version of the current page: switching from a privacy page stays on a privacy page. Arabic uses a right-to-left
layout. Page URLs remain usable directly, including after a reload.

## Homepage and navigation

The homepage uses Octoboard's original silver mechanical octopus, red eyes, and pixel wordmark on a dark surface.
It presents requests that span several projects and four benefits: reducing manual coordination between projects,
keeping independent native project sessions with their own rules, retaining different agent brands and toolchains,
and letting the user work directly with the project agent handling a task. The explanation connects these benefits
to transferring tasks and results between frontend, backend, and documentation projects, preserving local project
configuration, and explaining specific requirements or code details directly to the agent doing the work.
The different-brands benefit also includes assigning a step to another brand. A coordinator running Claude Code can give
image generation to Codex, because Claude Code does not generate images, and can give Codex the steps that drive a local
application or a browser, where Codex is stronger. The coordinator then continues from what Codex reports.

A statement about collaboration across repositories appears beside a static illustration connecting the three agent
brands to Octoboard's mascot. It leads into the three stages of coordination: give the goal to a coordinator, let
project agents work, and follow or intervene in their conversations. The workflow illustration follows one team
invitation request through three independent project sessions: Codex handles the frontend, Claude Code the backend,
and Grok Build the documentation. All three project cards remain visible, each with its directory, project instructions,
Skills and hooks, assigned task, and report. They appear in three columns on wider screens and stack on smaller screens.

The example combines invitation and team-joining screens, invitation endpoints and permission checks, and a matching
user guide. Each project reports to the coordinator; the summary below the projects represents the same coordinator
shown above them, gathering all three results into one feature delivery. These are fixed, localized examples,
explicitly labelled as an illustration. The diagram does not show live application activity, run agents, or submit work.
Its replay control restarts the demonstration.

The homepage then presents the source repository and MIT License, local data handling and planned remote support,
platform availability, and nine questions. Each question can be expanded or collapsed independently, with the first
expanded initially; several answers can remain open at once. The questions cover suitable tasks, configuration
loading across directories or through subagents, different agent brands, reporting and direct participation, product
understanding, platform availability, preparation and costs, data handling, and planned remote hosts. Configuration
loading is described as dependent on the agent and its tools. Product understanding comes from ongoing conversations,
project knowledge, and reports available to the session, not automatic training or permanent memory.

Supported agents are Claude Code, Codex, and Grok Build. The preparation answer states that users install the agent
CLIs they want to use. Octoboard is free and open source under the MIT License. The agents it runs are the user's
own, including one pointed at a model the user hosts.

The primary action in the header, hero, menu, and closing section uses the same localized download label and opens
the repository's GitHub Releases page. It does not link directly to an installer file. The hero identifies macOS
Apple Silicon, and both the hero and closing section state that the installer is coming soon. The download action
does not imply that a package has already been published. Other repository links open GitHub; the footer links to the
localized privacy page, the MIT License as the license terms, and the operator's email address.

The footer forms a compact centered group: the wordmark and localized tagline, followed by navigation links and
copyright. On small screens, the wordmark and tagline stack vertically.

The closing section displays the original combined mascot, wordmark, and slogan: a horizontal arrangement on wider
screens and a vertical arrangement on small screens. Decorative graphics do not act as slide or pagination controls.

The floating header stays visible while scrolling and links to benefits, the coordination explanation, and questions
from either page. Following a section link leaves space for the header above the destination. Its inline links
identify the current homepage section as the visitor moves through the page. The header has a glass surface with edge
refraction where supported and a sheen that follows the pointer. Browsers without refraction support use blurred glass;
those without backdrop blur use a more opaque surface.
A menu button is available at every screen width and opens a full-screen navigation dialog. The button changes into a
close control; the background cannot be interacted with or scrolled while the menu is open. Tab and Shift+Tab stay
within the dialog. Closing it with the button or Escape returns focus to the header's menu button. Choosing an internal
destination or another language closes the dialog as well.

## Brand slogan

The brand slogan is "The Programming Terminator". Its two intended meanings are the ambition to hand programming work
over to agents, and the image of a powerful Terminator in the programming world, reflected in the mechanical mascot.
It is not a pun on "terminal". The slogan expresses a vision and a brand identity; it does not claim that fully
autonomous development is already implemented.

## Platform availability

The platform section appears between the data-handling section and the questions. Its five entries have equal visual
weight in a desktop row and become a compact list of icons and text on small screens. The macOS entry uses brighter
text and identifies the version as developed, with an installer for Apple Silicon coming soon on GitHub Releases.
Completed development and a publicly available installer are separate states; the website currently promises the
latter only as forthcoming.

Status: not implemented — versions for Linux, Windows, iOS, and Android. Each appears with a platform icon and a
planned status, without a download or an availability date. The platform answer repeats the same availability status.

## Motion and accessibility

The hero text and mascot enter with a short motion. Red and silver pixel particles move through the surrounding
space without covering the content or intercepting interaction; they pause when the hero is out of view or the page
is in the background. Sections reveal as they enter view. The standalone collaboration statement reveals its words
in reading order as they reach the viewing area, with word boundaries appropriate to the selected language.
Assistive technology receives the complete statement rather than a sequence of separate animated words.

The workflow illustration runs one finite sequence as it enters view or the replay control is activated: the request
reaches the coordinator, tasks travel to all three project sessions, and execution and reporting are highlighted.
Three reports then reach the summary; it and the coordinator above highlight together to acknowledge receipt before
the shared feature delivery is highlighted. Brief signals travel along the connections, while card highlights hold
before fading softly. All tasks and reports remain readable throughout the sequence.

The illustration uses the application's working pulse and awaiting-instructions speech bubble, with visible labels
and accessible names. They retain the meanings in `docs/product/sessions.md`, "Session statuses"; awaiting instructions
is not a task-completion indicator. The example reports and shared delivery describe the outcome separately.
The sequence and working pulses pause offscreen or in the background; returning to view starts a new pass. A layout
change leaves the connections static instead of continuing a highlight along a changed route. The planned remote-host
illustration has a pulsing connection indicator. Question expansion and control state changes also have visual
transitions.

A reduced-motion preference disables these animations and transitions, hides the particles, and leaves the content
visible. The header keeps a static blurred glass surface without refraction or a moving sheen. Without JavaScript,
the pre-rendered homepage and privacy content remain readable, and questions can still be expanded and collapsed;
section reveals never leave that content hidden. Replaying the illustration and opening the menu require JavaScript;
the replay control is absent when motion is reduced or playback is unavailable. The header provides a skip link to the
main content, and interactive controls, including questions, show keyboard focus.

## Missing pages and errors

An unknown address receives a branded 404 page with the original mascot, an explanation, and a link back to the
homepage. The static GitHub Pages fallback is in English, works without JavaScript, and preserves the requested
missing address rather than replacing it with a different URL. Its return-home link opens the English homepage under
the site's deployment root.

Errors encountered within the running site use localized messages, distinguishing missing pages from other loading
errors. These error views also provide a return-home link. Error pages request exclusion from search indexing; the
static error route is not included in the sitemap.

## Privacy disclosures

The privacy page identifies BaiJunjie as the operator and gives `support@octoboard.dev` for privacy questions.
Its disclosures distinguish:

- application configuration and session data stored locally, with no upload of code, conversations, or usage data
  to developer-operated servers;
- third-party agents and tools, which can send data to their configured providers under those providers' policies;
- repository operations, including remote status checks, which can contact configured Git remotes;
- task information passed between agent sessions, which can consequently reach more than one provider;
- the static website, with no added analytics, advertising trackers, or cookies, and GitHub Pages' security logging
  of visitor IP addresses, with a link to GitHub's privacy statement;
- information voluntarily supplied through email or GitHub Issues, including the visibility of public issues.

The local-data explanation is not a promise that all workflow data remains on one device. There is no separate
website terms page; the software's license is linked separately from the privacy policy.

## Remote host support

Status: not implemented. The homepage and the privacy page say that remote host support is planned and do not describe
a workflow. A decorative illustration sits beside that status.
