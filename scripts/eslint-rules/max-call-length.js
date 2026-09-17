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
      tooLong: 'Function call exceeds {{ maxLength }} characters ({{ actualLength }}). Break arguments into multiple lines.'
    }
  },
  create(context) {
    const options = context.options[0] || {}
    const maxLength = options.maxLength || 100
    const sourceCode = context.getSourceCode()

    const isAlreadyMultiline = (node) => {
      return node.loc.start.line !== node.loc.end.line
    }

    const getLineLength = (node) => {
      const line = sourceCode.lines[node.loc.start.line - 1]

      return line.length
    }

    return {
      CallExpression(node) {
        if (isAlreadyMultiline(node)) {
          return
        }

        if (node.arguments.length === 0) {
          return
        }

        const lineLength = getLineLength(node)

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
            const openParen = sourceCode.getTokenAfter(
              node.callee,
              { filter: t => t.value === '(' }
            )
            
            const closeParen = sourceCode.getLastToken(node)

            if (!openParen || closeParen.value !== ')') {
              return null
            }

            const args = node.arguments.map(arg => sourceCode.getText(arg))
            const formattedArgs = args.map(arg => `${indent}  ${arg}`).join(',\n')

            return fixer.replaceTextRange(
              [openParen.range[0], closeParen.range[1]],
              `(\n${formattedArgs}\n${indent})`
            )
          }
        })
      }
    }
  }
}