import type { SessionProse } from "../terminalScripts";

import type { previewCopy as english } from "./en";

export const previewCopy: typeof english = {
  demo: "交互演示 · 示例数据",
  unavailable: "预览加载失败，请刷新页面重试。",
  workspace: "餐饮业务工作区",
  coordinator: "开发门店套餐促销功能",
  review: "审查套餐订单校验",
  projectAdmin: "促销配置后台",
  projectMerchant: "门店经营后台",
  projectCustomer: "顾客点餐网站",
  projectServer: "餐饮订单 API",
  projectMobile: "手机点餐应用",
  projectTablet: "店员点餐平板",
  projectPos: "门店收银台",
  projectKitchen: "后厨出餐屏",
  adminTask: "开发套餐价格、时间与门店配置功能",
  merchantTask: "开发上架、库存与核销管理功能",
  customerTask: "实现套餐浏览、下单与订单详情",
  serverTask: "实现订单校验与计价",
  mobileTask: "实现套餐浏览、下单与核销码展示",
  tabletTask: "实现桌台点餐、套餐加购与订单同步",
  posTask: "实现结账、优惠校验与核销",
  kitchenTask: "实现制作单与出餐状态同步",
  orderChecks: "测试库存限制与重复下单保护",
  promotionReportTitle: "门店套餐促销",
  progressReportSubtitle: "阶段进度汇总 · 示例数据",
  inProgress: "联调进行中",
  promotionProgressIntro:
    "八个项目正在开发套餐促销功能。这份阶段汇总集中展示已实现的改动、正在进行的联调检查，以及一项待确认的促销规则。",
  projects: "协作项目",
  checksPassedSoFar: "目前已通过的示例检查",
  decision: "待确认决策",
  adminResult: "已实现套餐价格、活动时间与适用门店的配置功能。",
  adminProgress: "配置校验检查已通过，等待联调反馈。",
  merchantResult: "已实现套餐上架、库存管理与订单核销功能。",
  merchantProgress: "正在联调售罄与已核销状态。",
  customerResult: "已实现套餐浏览、下单与订单详情。",
  customerProgress: "正在检查不适用门店与套餐售罄时的下单行为。",
  serverResult: "已实现活动资格校验、服务端计价、库存预留与重复下单保护。",
  serverProgress: "正在与客户端项目联调订单响应，审查测试持续在后台运行。",
  mobileResult: "已实现套餐浏览、下单与核销码展示。",
  mobileProgress: "正在检查手机上的售罄状态。",
  tabletResult: "已实现桌台点餐与套餐加购。",
  tabletProgress: "正在检查与其他项目的订单同步。",
  posResult: "已实现结账、优惠校验与核销功能。",
  posProgress: "结账与核销检查已通过，等待联调反馈。",
  kitchenResult: "已实现制作单与出餐状态更新。",
  kitchenProgress: "正在检查订单核销后的制作单更新。",
  stackingDecision:
    "套餐优惠是否可与会员折扣叠加？确认前，示例保持不叠加，其他开发与联调任务继续推进。",
  next: "下一步",
  promotionContinuation:
    "继续检查跨项目订单流程并汇总联调结果，规则确认后再更新优惠检查。",
  terminals: {
    "preview-coordinator": {
      prompt: "为点餐与门店系统开发套餐促销功能。",
      verb: "协调中",
      say: {
        plan: [
          "我正在协调八个项目的开发与联调。",
          "",
          "促销配置后台 → 套餐价格、活动时间与适用门店配置",
          "门店经营后台 → 上架、库存与核销管理",
          "顾客点餐网站 → 套餐浏览、下单与订单",
          "餐饮订单 API → 活动校验、计价与订单安全",
          "手机点餐应用 → 浏览、下单与核销码",
          "店员点餐平板 → 桌台点餐与套餐加购",
          "门店收银台 → 结账、优惠与核销",
          "后厨出餐屏 → 制作单与出餐状态",
        ],
        shared: [
          "已向各项目智能体同步活动与订单规则。",
          "每个会话沿用自己的项目指令。",
        ],
        status: [
          "促销配置后台：配置功能与校验已就绪。",
          "门店收银台：结账与核销检查已通过。",
          "其余六个项目仍在检查联调情况。",
          "目前已有 48 项示例检查通过。",
        ],
        decision: [
          "还需要确认一项促销规则：",
          "套餐优惠是否可与会员折扣叠加？",
          "示例暂时保持优惠不叠加。",
        ],
        pricesAgree: [
          "各客户端项目的订单价格目前保持一致。",
        ],
        collecting: [
          "正在收集联调结果并更新阶段进度汇总。",
        ],
      },
    },
    "preview-admin-session": {
      prompt: "开发价格、活动时间与适用门店配置。",
      verb: "处理中",
      say: {
        summary: [
          "发布活动前现在要求至少选择一家适用门店。",
          "活动结束时间必须晚于开始时间。",
          "已向其他项目同步促销规则。",
          "等待这部分功能的联调反馈。",
        ],
      },
    },
    "preview-merchant-session": {
      prompt: "开发上架、库存与核销管理。",
      verb: "检查中",
      say: {
        summary: [
          "套餐仅在有库存且处于活动时间内时才会上架。",
          "已添加到店核销查询。",
          "沿用促销配置后台的促销规则。",
        ],
        checking: [
          "检查售罄与已核销状态。",
        ],
      },
    },
    "preview-customer-session": {
      prompt: "开发套餐浏览、下单与订单。",
      verb: "检查中",
      say: {
        summary: [
          "结账时每个订单只使用一个请求编号，重复点击不会重复下单。",
          "订单展示价格由餐饮订单 API 计算。",
        ],
        checking: [
          "正在检查不适用门店与套餐售罄时的下单行为。",
        ],
      },
    },
    "preview-server-session": {
      prompt: "实现订单校验与计价。",
      verb: "检查中",
      say: {
        summary: [
          "订单按有效套餐计价，不再采用客户端提交的价格。",
          "扣库存与创建订单在同一事务中完成。",
          "已向其他项目同步订单接口约定。",
        ],
        checking: [
          "正在与客户端项目联调订单响应。",
        ],
      },
    },
    "preview-review": {
      prompt: "审查套餐订单功能的实现。",
      verb: "审查中",
      say: {
        finding: [
          "发现问题：餐饮订单 API 信任客户端提交的价格。",
          "修复：按适用且有效的套餐计算订单金额。",
        ],
        summary: [
          "已检查活动时间、门店校验、库存预留与重复请求。",
          "当前审查结果已准备好，可随时查看。",
          "pnpm test --watch 持续在后台运行。",
          "等待下一步审查指示。",
        ],
      },
    },
    "preview-order-checks": {
      prompt: "测试库存上限与重复下单保护。",
      verb: "测试中",
      say: {
        summary: [
          "还需要确认一项促销规则。",
          "其他会话继续按优惠不叠加的规则联调。",
        ],
      },
      question: {
        header: "折扣规则",
        text: "套餐优惠是否可与会员折扣叠加？",
        options: [
          { label: "不叠加", detail: "会员折扣不适用于套餐。" },
          { label: "允许叠加", detail: "两种优惠可用于同一订单。" },
          { label: "输入其他答案" },
        ],
      },
    },
    "preview-mobile-session": {
      prompt: "开发浏览、下单与核销码。",
      verb: "检查中",
      say: {
        summary: [
          "核销码只对其所属的已支付订单有效。",
          "重复点击会沿用同一个请求编号。",
          "展示价格由餐饮订单 API 计算。",
        ],
        checking: [
          "检查手机上的售罄状态。",
        ],
      },
    },
    "preview-tablet-session": {
      prompt: "开发桌台点餐与套餐同步。",
      verb: "检查中",
      say: {
        summary: [
          "每项加购都会按套餐的数量上限校验。",
          "发送加购前会检测桌台是否已在其他设备上变更。",
          "沿用促销配置后台的促销规则。",
        ],
        checking: [
          "检查与其他项目的订单同步。",
        ],
      },
    },
    "preview-pos-session": {
      prompt: "开发结账、优惠校验与核销。",
      verb: "处理中",
      say: {
        summary: [
          "结账时套餐优惠现在会取代会员折扣，而不是叠加。",
          "服务端未确认的核销会如实提示。",
          "结账代码采用餐饮订单 API 的计价结果。",
          "等待这部分功能的联调反馈。",
        ],
      },
    },
    "preview-kitchen-session": {
      prompt: "开发制作单打印与出餐状态更新。",
      verb: "检查中",
      say: {
        summary: [
          "打印状态现在跟随门店网关的队列与打印机。",
          "制作单在其他设备上变更时，更新前会先提示。",
        ],
        checking: [
          "正在检查订单核销后的制作单更新。",
        ],
      },
    },
  } as Record<string, SessionProse>,
};
