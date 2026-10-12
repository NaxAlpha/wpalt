// Browser previews only. Rust validates declarations and recomputes submissions.
export function matches(rule, values) {
  const value = values[rule.field];
  switch (rule.operation) {
    case "Equal":
      return Object.hasOwn(values, rule.field) && value === rule.value;
    case "Present":
      return (
        Object.hasOwn(values, rule.field) &&
        value !== null &&
        (typeof value !== "string" || value.trim() !== "")
      );
    case "OneOf":
      return Object.hasOwn(values, rule.field) && rule.values.includes(value);
    case "Greater":
      return typeof value === "number" && value > rule.value;
    case "AtLeast":
      return typeof value === "number" && value >= rule.value;
    case "Less":
      return typeof value === "number" && value < rule.value;
    case "AtMost":
      return typeof value === "number" && value <= rule.value;
    case "All":
      return rule.conditions.every((child) => matches(child, values));
    case "Any":
      return rule.conditions.some((child) => matches(child, values));
    case "Not":
      return !matches(rule.condition, values);
    default:
      throw new Error("This condition needs an updated form definition.");
  }
}
function score(fields, values, name) {
  return fields
    .find((field) => field.name === name)
    ?.widget?.options?.find((option) => option.value === values[name])?.score;
}
function finite(number) {
  if (!Number.isFinite(number))
    throw new Error("A calculation exceeded its numeric range.");
  return number;
}
function expression(node, fields, values) {
  if (node.operation === "If")
    return expression(
      matches(node.condition, values) ? node.then : node.otherwise,
      fields,
      values,
    );
  if (node.operation === "Literal") return finite(node.value);
  if (node.operation === "Field")
    return typeof values[node.field] === "number"
      ? finite(values[node.field])
      : undefined;
  if (node.operation === "Score") return score(fields, values, node.field);
  if (node.operands) {
    const items = [];
    for (const child of node.operands) {
      const value = expression(child, fields, values);
      if (value === undefined) return undefined;
      items.push(value);
    }
    switch (node.operation) {
      case "Add":
        return finite(items.reduce((a, b) => a + b, 0));
      case "Multiply":
        return finite(items.reduce((a, b) => a * b, 1));
      case "Min":
        return finite(Math.min(...items));
      case "Max":
        return finite(Math.max(...items));
      case "Average":
        return finite(items.reduce((a, b) => a + b / items.length, 0));
    }
  }
  if (node.left) {
    const a = expression(node.left, fields, values);
    if (a === undefined) return undefined;
    const b = expression(node.right, fields, values);
    if (b === undefined) return undefined;
    if (node.operation === "Divide" && b === 0)
      throw new Error("A calculation cannot divide by zero.");
    return finite(node.operation === "Subtract" ? a - b : a / b);
  }
  const value = expression(node.operand, fields, values);
  if (value === undefined) return undefined;
  switch (node.operation) {
    case "Abs":
      return Math.abs(value);
    case "Floor":
      return Math.floor(value);
    case "Ceil":
      return Math.ceil(value);
    case "Round": {
      const scale = 10 ** node.decimals;
      const scaled = finite(value * scale);
      // Math.round observes the real fraction. Adding .5 first can round a
      // value just below a tie up to the tie itself in binary64.
      const rounded = Math.sign(scaled) * Math.round(Math.abs(scaled));
      return finite(rounded / scale);
    }
    default:
      throw new Error("This calculation needs an updated form definition.");
  }
}
export function calculated(calculation, fields, values) {
  if (calculation.operation === "Expression")
    return expression(calculation.expression, fields, values);
  const items = calculation.fields.map((name) =>
    calculation.operation === "Score"
      ? score(fields, values, name)
      : values[name],
  );
  if (
    !items.every((value) => typeof value === "number" && Number.isFinite(value))
  )
    return undefined;
  return finite(
    calculation.operation === "Product"
      ? items.reduce((a, b) => a * b, 1)
      : items.reduce((a, b) => a + b, 0),
  );
}
export function evaluated(fields, input) {
  const values = {},
    visible = [],
    errors = {};
  for (const field of fields) {
    if (field.visible_when && !matches(field.visible_when, values)) continue;
    visible.push(field);
    if (field.calculation) {
      try {
        const result = calculated(field.calculation, fields, values);
        if (result !== undefined) values[field.name] = result;
      } catch (error) {
        errors[field.name] = error.message;
      }
    } else if (Object.hasOwn(input, field.name))
      values[field.name] = input[field.name];
  }
  return { values, visible, errors };
}
