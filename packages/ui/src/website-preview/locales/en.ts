import type { SessionProse } from "../terminalScripts";

export const previewCopy = {
  demo: "Interactive demo · Sample data",
  unavailable: "The preview could not load. Reload the page to try again.",
  workspace: "Bistro workspace",
  coordinator: "Develop meal deal promotion features",
  review: "Review meal deal order validation",
  projectAdmin: "Promotion console",
  projectMerchant: "Store management console",
  projectCustomer: "Customer ordering website",
  projectServer: "Restaurant order API",
  projectMobile: "Mobile ordering app",
  projectTablet: "Staff ordering tablet",
  projectPos: "Checkout register",
  projectKitchen: "Kitchen display",
  adminTask: "Build price, date and store configuration",
  merchantTask: "Build listing, stock and redemption management",
  customerTask: "Build browsing, checkout and orders",
  serverTask: "Implement order validation and pricing",
  mobileTask: "Build browsing, ordering and redemption codes",
  tabletTask: "Build table ordering and meal deal sync",
  posTask: "Build checkout, discount validation and redemption",
  kitchenTask: "Build ticket printing and food-ready updates",
  orderChecks: "Test stock limits and duplicate-order protection",
  promotionReportTitle: "Meal deal promotion",
  progressReportSubtitle: "Progress report · Sample data",
  inProgress: "Integration in progress",
  promotionProgressIntro:
    "Eight projects are developing meal deal promotion features. This progress report brings together implemented changes, ongoing integration checks and one promotion rule awaiting confirmation.",
  projects: "Projects coordinated",
  checksPassedSoFar: "Sample checks passed so far",
  decision: "Decision to confirm",
  adminResult:
    "Implemented controls for meal deal prices, promotion dates and eligible stores.",
  adminProgress:
    "Configuration validation checks passed. Awaiting integration feedback.",
  merchantResult:
    "Implemented listing, stock management and order-redemption features.",
  merchantProgress:
    "Checking sold-out and already-redeemed states during integration.",
  customerResult: "Implemented meal deal browsing, checkout and order details.",
  customerProgress:
    "Checking checkout behavior for unavailable stores and sold-out offers.",
  serverResult:
    "Implemented eligibility checks, server pricing, stock reservation and duplicate-order protection.",
  serverProgress:
    "Checking order responses with the client projects. Review tests continue in the background.",
  mobileResult:
    "Implemented meal deal browsing, ordering and redemption code display.",
  mobileProgress: "Checking sold-out states on a phone.",
  tabletResult: "Implemented table ordering and meal deal add-ons.",
  tabletProgress: "Checking order sync with the other projects.",
  posResult: "Implemented checkout, discount validation and redemption features.",
  posProgress:
    "Checkout and redemption checks passed. Awaiting integration feedback.",
  kitchenResult: "Implemented preparation tickets and food-ready updates.",
  kitchenProgress: "Checking ticket updates after order redemption.",
  stackingDecision:
    "Can the meal deal be combined with a member discount? The sample keeps them separate while you decide. The other implementation and integration tasks continue.",
  next: "Next step",
  promotionContinuation:
    "Continue checking the order flow across projects and collecting integration results. Update the discount checks once the rule is confirmed.",
  terminals: {
    "preview-coordinator": {
      prompt: "Add meal deal promotion features to our ordering and store systems.",
      verb: "Coordinating",
      say: {
        plan: [
          "I am coordinating implementation and integration across eight projects.",
          "",
          "Promotion console → price, date and store configuration",
          "Store management console → listing, stock and redemption",
          "Customer ordering website → browsing, checkout and orders",
          "Restaurant order API → eligibility, pricing and order safety",
          "Mobile ordering app → browsing, ordering and redemption codes",
          "Staff ordering tablet → table orders and meal deal add-ons",
          "Checkout register → checkout, discounts and redemption",
          "Kitchen display → tickets and food-ready status",
        ],
        shared: [
          "Shared promotion and order rules with all agents.",
          "Each session keeps its own project instructions.",
        ],
        status: [
          "Promotion console: configuration controls and validation are ready.",
          "Checkout register: checkout and redemption checks have passed.",
          "The other six projects are still checking their integration.",
          "48 sample checks have passed so far.",
        ],
        decision: [
          "One promotion rule remains to be confirmed:",
          "can meal deals be combined with member discounts?",
          "The sample keeps these discounts separate meanwhile.",
        ],
        pricesAgree: [
          "Order prices agree across the client projects so far.",
        ],
        collecting: [
          "Collecting integration results and updating the progress report.",
        ],
      },
    },
    "preview-admin-session": {
      prompt: "Build price, date and store configuration.",
      verb: "Working",
      say: {
        summary: [
          "Publishing now needs at least one eligible store.",
          "The promotion must end after it starts.",
          "Shared the promotion rules with the other projects.",
          "Awaiting feedback on this part of the integration.",
        ],
      },
    },
    "preview-merchant-session": {
      prompt: "Build listing, stock and redemption management.",
      verb: "Checking",
      say: {
        summary: [
          "An offer is listed only while it has stock and is inside its promotion window.",
          "In-store redemption lookup is added.",
          "Using the promotion rules set in the promotion console.",
        ],
        checking: [
          "Checking sold-out and already-redeemed states.",
        ],
      },
    },
    "preview-customer-session": {
      prompt: "Build browsing, checkout and orders.",
      verb: "Checking",
      say: {
        summary: [
          "Checkout keeps one request ID per order, so a repeated tap cannot order twice.",
          "The displayed order price comes from the restaurant order API.",
        ],
        checking: [
          "Checking checkout behavior for unavailable stores and sold-out offers.",
        ],
      },
    },
    "preview-server-session": {
      prompt: "Implement order validation and pricing.",
      verb: "Checking",
      say: {
        summary: [
          "Orders are priced from the active offer, never from the client.",
          "Stock reservation and order creation share one transaction.",
          "Shared the order contract with the other projects.",
        ],
        checking: [
          "Checking order responses with the client projects.",
        ],
      },
    },
    "preview-review": {
      prompt: "Review the meal deal order implementation.",
      verb: "Reviewing",
      say: {
        finding: [
          "Finding: the service trusted the client price.",
          "Fix: price the order from an eligible active offer.",
        ],
        summary: [
          "Checked promotion dates, store eligibility, stock reservation and repeated requests.",
          "Current review findings are ready for inspection.",
          "pnpm test --watch continues in the background.",
          "Awaiting the next review instruction.",
        ],
      },
    },
    "preview-order-checks": {
      prompt: "Test stock limits and duplicate-order protection.",
      verb: "Testing",
      say: {
        summary: [
          "One promotion rule still needs confirmation.",
          "Other sessions continue integration with these discounts kept separate.",
        ],
      },
      question: {
        header: "Discount rule",
        text: "Can meal deals be combined with member discounts?",
        options: [
          { label: "Keep them separate", detail: "A member discount does not apply to a meal deal." },
          { label: "Allow combining", detail: "Both discounts apply to the same order." },
          { label: "Type something else" },
        ],
      },
    },
    "preview-mobile-session": {
      prompt: "Build browsing, ordering and redemption codes.",
      verb: "Checking",
      say: {
        summary: [
          "A redemption code is valid only for its own paid order.",
          "Repeated taps reuse one request ID.",
          "The displayed price comes from the restaurant order API.",
        ],
        checking: [
          "Checking sold-out offers on a phone.",
        ],
      },
    },
    "preview-tablet-session": {
      prompt: "Build table ordering and meal deal sync.",
      verb: "Checking",
      say: {
        summary: [
          "Each extra is checked against its package's limit.",
          "A table changed on another device is detected before an add-on is sent.",
          "Using the promotion rules set in the promotion console.",
        ],
        checking: [
          "Checking order sync with the other projects.",
        ],
      },
    },
    "preview-pos-session": {
      prompt: "Build checkout, discount validation and redemption.",
      verb: "Working",
      say: {
        summary: [
          "A meal deal now replaces a member discount at checkout instead of stacking with it.",
          "A redemption the server does not confirm is reported.",
          "The checkout code uses the price from the restaurant order API.",
          "Awaiting feedback on this part of the integration.",
        ],
      },
    },
    "preview-kitchen-session": {
      prompt: "Build ticket printing and food-ready updates.",
      verb: "Checking",
      say: {
        summary: [
          "Print status now follows the store gateway's queue and printer.",
          "A ticket changed on another device is flagged before it is updated.",
        ],
        checking: [
          "Checking ticket updates after order redemption.",
        ],
      },
    },
  } as Record<string, SessionProse>,
};
