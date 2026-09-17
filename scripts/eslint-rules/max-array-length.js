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
      tooLong: 'Array exceeds {{ maxLength }} characters ({{ actualLength }}). Break into multiple lines with one element per line.'
    }
  },
  create(context) {
    const options = context.options[0] || {}
    const maxLength = options.maxLength || 100
    const sourceCode = context.getSourceCode()

    return {
      ArrayExpression(node) {
        if (node.elements.length === 0) {
          return
        }

        if (node.loc.start.line !== node.loc.end.line) {
          return
        }

        const lineLength = sourceCode.lines[node.loc.start.line - 1].length

        if (lineLength <= maxLength) {
          return
        }

        context.report({
          node,
          messageId: 'tooLong',
          data: {
            maxLength,
            actualLength: lineLength
          },
          fix(fixer) {
            const indent = ' '.repeat(node.loc.start.column)
            const openBracket = sourceCode.getFirstToken(node)
            const closeBracket = sourceCode.getLastToken(node)

            const elements = node.elements.map(el => {
              if (el === null) {
                return ''
              }

              return sourceCode.getText(el)
            })

            const formattedElements = elements.map(el => `${indent}  ${el}`).join(',\n')

            return fixer.replaceTextRange(
              [openBracket.range[0], closeBracket.range[1]],
              `[\n${formattedElements}\n${indent}]`
            )
          }
        })
      }
    }
  }
}