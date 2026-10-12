import { h } from "preact";
import { Button, Field, SelectField as Select } from "./ui.jsx";

const leaf = (fields) => ({
  operation: "Present",
  field: fields[0]?.name || "",
});
const literal = (value = 0) => ({ operation: "Literal", value });
const numberKinds = ["number", "integer"];
function typed(raw, field) {
  return field?.schema.kind === "boolean"
    ? raw === "true"
    : numberKinds.includes(field?.schema.kind)
      ? raw === ""
        ? raw
        : Number(raw)
      : raw;
}
function condition(operation, fields, previous) {
  const field = previous?.field || fields[0]?.name || "";
  if (operation === "All" || operation === "Any")
    return {
      operation,
      conditions: previous?.conditions || [previous || leaf(fields)],
    };
  if (operation === "Not")
    return {
      operation,
      condition: previous?.condition || previous || leaf(fields),
    };
  if (operation === "Present") return { operation, field };
  if (operation === "OneOf")
    return {
      operation,
      field,
      values: [
        typed(
          "",
          fields.find((item) => item.name === field),
        ),
      ],
    };
  return {
    operation,
    field,
    value:
      operation === "Equal"
        ? typed(
            "",
            fields.find((item) => item.name === field),
          )
        : 0,
  };
}
export function ConditionEditor({
  value,
  fields,
  update,
  depth = 1,
  optional = false,
}) {
  const group = value && ["All", "Any", "Not"].includes(value.operation);
  const selected = fields.find((field) => field.name === value?.field);
  return (
    <fieldset>
      <legend>{depth === 1 ? "Visibility rule" : "Nested rule"}</legend>
      {!group && (
        <Select
          label={
            depth === 1
              ? "Show when a previous field has a value"
              : "Rule field"
          }
          value={value?.field || ""}
          onChange={(event) =>
            update(
              event.currentTarget.value
                ? {
                    ...(value || leaf(fields)),
                    field: event.currentTarget.value,
                  }
                : null,
            )
          }
        >
          {optional && <option value="">Always visible</option>}
          {fields.map((field) => (
            <option key={field.name} value={field.name}>
              {field.schema.label || field.name}
            </option>
          ))}
        </Select>
      )}
      {value && (
        <>
          <Select
            label={depth === 1 ? "Condition" : "Rule type"}
            value={value.operation}
            onChange={(event) =>
              update(condition(event.currentTarget.value, fields, value))
            }
          >
            <option value="Present">Has a value</option>
            <option value="Equal">Equals a value</option>
            <option value="OneOf">Matches one of the values</option>
            <option value="Greater">Greater than</option>
            <option value="AtLeast">At least</option>
            <option value="Less">Less than</option>
            <option value="AtMost">At most</option>
            {depth < 8 && (
              <>
                <option value="All">All rules match</option>
                <option value="Any">Any rule matches</option>
                <option value="Not">Rule does not match</option>
              </>
            )}
          </Select>
          {["Equal", "Greater", "AtLeast", "Less", "AtMost"].includes(
            value.operation,
          ) && (
            <Field
              label="Value to match"
              value={String(value.value)}
              type={
                value.operation !== "Equal" ||
                numberKinds.includes(selected?.schema.kind)
                  ? "number"
                  : "text"
              }
              step="any"
              onInput={(event) =>
                update({
                  ...value,
                  value:
                    value.operation === "Equal"
                      ? typed(event.currentTarget.value, selected)
                      : Number(event.currentTarget.value),
                })
              }
            />
          )}
          {value.operation === "OneOf" && (
            <>
              {value.values.map((item, index) => (
                <div key={index}>
                  <Field
                    label={`Matching value ${index + 1}`}
                    value={String(item)}
                    onInput={(event) =>
                      update({
                        ...value,
                        values: value.values.map((old, i) =>
                          i === index
                            ? typed(event.currentTarget.value, selected)
                            : old,
                        ),
                      })
                    }
                  />
                  <Button
                    variant="quiet"
                    disabled={value.values.length === 1}
                    onClick={() =>
                      update({
                        ...value,
                        values: value.values.filter((_, i) => i !== index),
                      })
                    }
                  >
                    Remove matching value
                  </Button>
                </div>
              ))}
              <Button
                variant="secondary"
                disabled={value.values.length >= 32}
                onClick={() =>
                  update({ ...value, values: [...value.values, ""] })
                }
              >
                Add matching value
              </Button>
            </>
          )}
          {value.conditions?.map((child, index) => (
            <div key={index}>
              <ConditionEditor
                value={child}
                fields={fields}
                depth={depth + 1}
                update={(changed) =>
                  update({
                    ...value,
                    conditions: value.conditions.map((old, i) =>
                      i === index ? changed : old,
                    ),
                  })
                }
              />
              <Button
                variant="quiet"
                disabled={value.conditions.length === 1}
                onClick={() =>
                  update({
                    ...value,
                    conditions: value.conditions.filter((_, i) => i !== index),
                  })
                }
              >
                Remove rule
              </Button>
            </div>
          ))}
          {value.conditions && (
            <Button
              variant="secondary"
              disabled={value.conditions.length >= 32 || depth >= 8}
              onClick={() =>
                update({
                  ...value,
                  conditions: [...value.conditions, leaf(fields)],
                })
              }
            >
              Add rule
            </Button>
          )}
          {value.operation === "Not" && (
            <ConditionEditor
              value={value.condition}
              fields={fields}
              depth={depth + 1}
              update={(child) => update({ ...value, condition: child })}
            />
          )}
          {optional && (
            <Button variant="quiet" onClick={() => update(null)}>
              Make always visible
            </Button>
          )}
        </>
      )}
    </fieldset>
  );
}
function expression(operation, fields) {
  if (operation === "Literal") return literal();
  if (operation === "Field" || operation === "Score")
    return {
      operation,
      field:
        fields.find((field) =>
          operation === "Score"
            ? field.widget?.kind === "Choice"
            : numberKinds.includes(field.schema.kind),
        )?.name || "",
    };
  if (["Add", "Multiply", "Min", "Max", "Average"].includes(operation))
    return { operation, operands: [literal(), literal()] };
  if (operation === "Subtract" || operation === "Divide")
    return { operation, left: literal(), right: literal(1) };
  if (operation === "If")
    return {
      operation,
      condition: leaf(fields),
      then: literal(),
      otherwise: literal(),
    };
  return {
    operation,
    operand: literal(),
    ...(operation === "Round" ? { decimals: 0 } : {}),
  };
}
export function ExpressionEditor({
  value,
  fields,
  update,
  depth = 1,
  label = "Calculation expression",
}) {
  const operations = {
    Literal: "Fixed number",
    Field: "Earlier number",
    Score: "Earlier choice score",
    Add: "Add",
    Subtract: "Subtract",
    Multiply: "Multiply",
    Divide: "Divide",
    Min: "Minimum",
    Max: "Maximum",
    Average: "Average",
    Abs: "Absolute value",
    Floor: "Round down",
    Ceil: "Round up",
    Round: "Round to decimals",
    If: "Conditional result",
  };
  return (
    <fieldset>
      <legend>{label}</legend>
      <Select
        label="Expression operation"
        value={value.operation}
        onChange={(event) =>
          update(expression(event.currentTarget.value, fields))
        }
      >
        {Object.entries(operations)
          .filter(
            ([operation]) =>
              depth < 8 || ["Literal", "Field", "Score"].includes(operation),
          )
          .map(([operation, title]) => (
            <option key={operation} value={operation}>
              {title}
            </option>
          ))}
      </Select>
      {value.operation === "Literal" && (
        <Field
          label="Fixed number"
          type="number"
          step="any"
          value={value.value}
          onInput={(event) =>
            update({
              ...value,
              value:
                event.currentTarget.value === ""
                  ? ""
                  : Number(event.currentTarget.value),
            })
          }
        />
      )}
      {value.field !== undefined && (
        <Select
          label="Expression field"
          value={value.field}
          onChange={(event) =>
            update({ ...value, field: event.currentTarget.value })
          }
        >
          {fields
            .filter((field) =>
              value.operation === "Score"
                ? field.widget?.kind === "Choice"
                : numberKinds.includes(field.schema.kind),
            )
            .map((field) => (
              <option key={field.name} value={field.name}>
                {field.schema.label || field.name}
              </option>
            ))}
        </Select>
      )}
      {value.operands?.map((child, index) => (
        <div key={index}>
          <ExpressionEditor
            value={child}
            fields={fields}
            depth={depth + 1}
            label={`Term ${index + 1}`}
            update={(changed) =>
              update({
                ...value,
                operands: value.operands.map((old, i) =>
                  i === index ? changed : old,
                ),
              })
            }
          />
          <Button
            variant="quiet"
            disabled={value.operands.length === 1}
            onClick={() =>
              update({
                ...value,
                operands: value.operands.filter((_, i) => i !== index),
              })
            }
          >
            Remove term
          </Button>
        </div>
      ))}
      {value.operands && (
        <Button
          variant="secondary"
          disabled={value.operands.length >= 32 || depth >= 8}
          onClick={() =>
            update({ ...value, operands: [...value.operands, literal()] })
          }
        >
          Add term
        </Button>
      )}
      {["left", "right", "operand", "then", "otherwise"]
        .filter((key) => value[key])
        .map((key) => (
          <ExpressionEditor
            key={key}
            value={value[key]}
            fields={fields}
            depth={depth + 1}
            label={
              {
                left: "First term",
                right: "Second term",
                operand: "Number to transform",
                then: "When rule matches",
                otherwise: "Otherwise",
              }[key]
            }
            update={(changed) => update({ ...value, [key]: changed })}
          />
        ))}
      {value.operation === "Round" && (
        <Field
          label="Decimal places"
          type="number"
          min="0"
          max="8"
          step="1"
          value={value.decimals}
          onInput={(event) =>
            update({ ...value, decimals: Number(event.currentTarget.value) })
          }
        />
      )}
      {value.condition && (
        <ConditionEditor
          value={value.condition}
          fields={fields}
          depth={depth + 1}
          update={(changed) => update({ ...value, condition: changed })}
        />
      )}
    </fieldset>
  );
}
