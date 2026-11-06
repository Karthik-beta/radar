export function collect(node, path = [], inheritedType = undefined, out = []) {
  const type = node.$type ?? inheritedType;
  if ('$value' in node) {
    out.push({ path, type, value: node.$value, description: node.$description ?? '' });
    return out;
  }
  for (const [key, child] of Object.entries(node)) {
    if (!key.startsWith('$')) collect(child, [...path, key], type, out);
  }
  return out;
}

export const formatters = {
  color: (v) => v,
  dimension: (v) => `${v.value}${v.unit}`,
  duration: (v) => `${v.value}${v.unit}`,
  fontFamily: (v) => v.map((f) => (f.includes(' ') ? `"${f}"` : f)).join(', '),
  fontWeight: (v) => String(v),
  cubicBezier: (v) => `cubic-bezier(${v.join(', ')})`,
};

export const format = (t) => {
  const f = formatters[t.type];
  if (!f) throw new Error(`no formatter for $type "${t.type}" at ${t.path.join('.')}`);
  return f(t.value);
};
