export function useSiteCopy() {
  const { t: translate } = useI18n();
  const config = useRuntimeConfig();
  const t = (key: string, values: Record<string, string | number> = {}) =>
    translate(key, {
      appName: config.public.appName,
      operatorName: config.public.operatorName,
      ...values,
    });
  return { t };
}
