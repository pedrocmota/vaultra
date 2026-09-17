module.exports = {
  meta: {
    type: 'layout',
    fixable: 'whitespace',
    schema: [
      {
        type: 'object',
        properties: {
          maxLength: {
            type: 'number'
          }
        },
        additionalProperties: false
      }
    ],
    messages: {
      tooLong: 'If condition exceeds {{ maxLength }} characters ({{ actualLength }}). Break into multiple lines with one condition per line.'
    }
  },
  create(context) {
    const options = context.options[0] || {}
    const maxLength = options.maxLength || 80
    const sourceCode = context.getSourceCode()

    const getConditionText = (node) => {
      const openParen = sourceCode.getTokenAfter(
        sourceCode.getFirstToken(node)
      )
      
      const closeParen = sourceCode.getTokenBefore(node.consequent)

      return sourceCode.text.slice(openParen.range[1], closeParen.range[0]).trim()
    }

    const isMultilineCondition = (node) => {
      const openParen = sourceCode.getTokenAfter(
        sourceCode.getFirstToken(node)
      )
      
      const closeParen = sourceCode.getTokenBefore(node.consequent)

      return openParen.loc.start.line !== closeParen.loc.end.line
    }

    const hasOneConditionPerLine = (node) => {
      const test = node.test

      if (test.type !== 'LogicalExpression') {
        return true
      }

      const collectConditions = (expr, conditions = []) => {
        if (expr.type === 'LogicalExpression') {
          collectConditions(expr.left, conditions)
          collectConditions(expr.right, conditions)
        } else {
          conditions.push(expr)
        }

        return conditions
      }

      const conditions = collectConditions(test)
      const lines = new Set()

      for (const cond of conditions) {
        const line = cond.loc.start.line

        if (lines.has(line)) {
          return false
        }

        lines.add(line)
      }

      return true
    }

    const formatConditions = (node) => {
      const test = node.test
      const indent = ' '.repeat(node.loc.start.column)

      if (test.type !== 'LogicalExpression') {
        return null
      }

      const collectParts = (expr, parts = [], operators = []) => {
        if (expr.type === 'LogicalExpression') {
          collectParts(expr.left, parts, operators)
          operators.push(expr.operator)
          collectParts(expr.right, parts, operators)
        } else {
          parts.push(sourceCode.getText(expr))
        }

        return { parts, operators }
      }

      const { parts, operators } = collectParts(test)

      if (parts.length < 2) {
        return null
      }

      let result = `\n${indent}  ${parts[0]}`

      for (let i = 1; i < parts.length; i++) {
        result += `\n${indent}  ${operators[i - 1]} ${parts[i]}`
      }

      result += `\n${indent}`

      return result
    }

    return {
      IfStatement(node) {
        const conditionText = getConditionText(node)
        const conditionLength = conditionText.length

        if (conditionLength <= maxLength) {
          return
        }

        if (isMultilineCondition(node) && hasOneConditionPerLine(node)) {
          return
        }

        context.report({
          node: node.test,
          messageId: 'tooLong',
          data: {
            maxLength,
            actualLength: conditionLength
          },
          fix(fixer) {
            const formatted = formatConditions(node)

            if (!formatted) {
              return null
            }

            const openParen = sourceCode.getTokenAfter(
              sourceCode.getFirstToken(node)
            )
            
            const closeParen = sourceCode.getTokenBefore(node.consequent)

            return fixer.replaceTextRange(
              [openParen.range[1], closeParen.range[0]],
              formatted
            )
          }
        })
      }
    }
  }
}