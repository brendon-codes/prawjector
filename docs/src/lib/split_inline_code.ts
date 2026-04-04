export const split_inline_code = (text: string) =>
  text.split("`").map((part, index) => ({
    key: `inline_code_${index}_${part.length}`,
    is_code: index % 2 === 1,
    text: part,
  }))
