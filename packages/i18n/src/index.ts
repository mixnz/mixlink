// Keyed UI strings with named placeholders (ADR-0003).

export type Params = Record<string, string | number>;

const PLACEHOLDER = /\{(\w+)\}/g;

/**
 * Returns `t(key, params)` for one language dictionary. A missing param stays visible as `{name}`;
 * a missing key (only possible through unchecked casts) shows the key itself.
 */
export function createTranslator<D extends Record<string, string>>(dict: D) {
  return (key: keyof D & string, params: Params = {}): string => {
    const template = dict[key] ?? key;
    return template.replace(PLACEHOLDER, (match: string, name: string) => {
      const value = params[name];
      return value === undefined ? match : String(value);
    });
  };
}
