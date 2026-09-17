module.exports = {
  meta: {
    type: 'layout',
    fixable: 'whitespace',
    messages: {
      paddedBlockStart: 'Block must not start with a blank line.',
      paddedBlockEnd: 'Block must not end with a blank line.'
    }
  },
  create(context) {
    const sourceCode = context.getSourceCode()

    const checkBlock = (node, body, openBrace, closeBrace) => {
      if (!body || body.length === 0) {
        return
      }

      const firstStatement = body[0]
      const lastStatement = body[body.length - 1]

      const openBraceToken = openBrace || sourceCode.getFirstToken(node, {filter: (t) => t.value === '{'})
      const closeBraceToken = closeBrace || sourceCode.getLastToken(node, {filter: (t) => t.value === '}'})

      if (!openBraceToken || !closeBraceToken) {
        return
      }

      const firstToken = sourceCode.getFirstToken(firstStatement)
      const textAfterOpen = sourceCode.text.slice(openBraceToken.range[1], firstToken.range[0])

      if (/\n\s*\n/.test(textAfterOpen)) {
        context.report({
          node: firstStatement,
          messageId: 'paddedBlockStart',
          fix(fixer) {
            return fixer.replaceTextRange(
              [openBraceToken.range[1], firstToken.range[0]],
              '\n' + ' '.repeat(firstToken.loc.start.column)
            )
          }
        })
      }

      const lastToken = sourceCode.getLastToken(lastStatement)
      const textBeforeClose = sourceCode.text.slice(lastToken.range[1], closeBraceToken.range[0])

      if (/\n\s*\n/.test(textBeforeClose)) {
        context.report({
          node: lastStatement,
          messageId: 'paddedBlockEnd',
          fix(fixer) {
            return fixer.replaceTextRange(
              [lastToken.range[1], closeBraceToken.range[0]],
              '\n' + ' '.repeat(closeBraceToken.loc.start.column)
            )
          }
        })
      }
    }

    return {
      BlockStatement(node) {
        checkBlock(node, node.body)
      },
      ClassBody(node) {
        checkBlock(node, node.body)
      },
      SwitchStatement(node) {
        if (node.cases.length === 0) {
          return
        }

        const openBrace = sourceCode.getTokenAfter(node.discriminant, {filter: (t) => t.value === '{'})
        const closeBrace = sourceCode.getLastToken(node)

        checkBlock(node, node.cases, openBrace, closeBrace)
      }
    }
  }
}