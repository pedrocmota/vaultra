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
      tooLong: 'Function parameters exceed {{ maxLength }} characters ({{ actualLength }}). Break into multiple lines with one parameter per line.'
    }
  },
  create(context) {
    const options = context.options[0] || {}
    const maxLength = options.maxLength || 100
    const sourceCode = context.getSourceCode()

    const checkParams = (node, params) => {
      if (params.length === 0) {
        return
      }

      let openParen = null
      let closeParen = null

      if (node.type === 'ArrowFunctionExpression') {
        const firstToken = sourceCode.getFirstToken(node)

        if (firstToken.value === '(') {
          openParen = firstToken
        } else if (firstToken.value === 'async') {
          openParen = sourceCode.getTokenAfter(firstToken, { filter: t => t.value === '(' })
        } else {
          return
        }

        if (openParen) {
          closeParen = sourceCode.getTokenBefore(node.body, { filter: t => t.value === ')' })
        }
      } else {
        openParen = sourceCode.getTokenBefore(params[0], { filter: t => t.value === '(' })
        closeParen = sourceCode.getTokenAfter(
          params[params.length - 1],
          { filter: t => t.value === ')' }
        )
      }

      if (!openParen || !closeParen) {
        return
      }

      if (openParen.loc.start.line !== closeParen.loc.end.line) {
        return
      }

      const lineLength = sourceCode.lines[openParen.loc.start.line - 1].length

      if (lineLength <= maxLength) {
        return
      }

      context.report({
        loc: {
          start: openParen.loc.start,
          end: closeParen.loc.end
        },
        messageId: 'tooLong',
        data: {
          maxLength,
          actualLength: lineLength
        },
        fix(fixer) {
          const indent = ' '.repeat(openParen.loc.start.column)
          const paramsFormatted = params.map(param => {
            return `${indent}  ${sourceCode.getText(param)}`
          }).join(',\n')

          return fixer.replaceTextRange(
            [openParen.range[0], closeParen.range[1]],
            `(\n${paramsFormatted}\n${indent})`
          )
        }
      })
    }

    return {
      FunctionDeclaration(node) {
        checkParams(node, node.params)
      },
      FunctionExpression(node) {
        checkParams(node, node.params)
      },
      ArrowFunctionExpression(node) {
        checkParams(node, node.params)
      }
    }
  }
}