module.exports = {
  meta: {
    type: 'layout',
    fixable: 'whitespace',
    messages: {
      noBlankAfterOpen: 'No blank line allowed after opening brace.',
      noBlankBeforeClose: 'No blank line allowed before closing brace.',
      noBlankBetweenProperties: 'No blank line allowed between object properties.'
    }
  },
  create(context) {
    const sourceCode = context.getSourceCode()

    const checkObject = (node) => {
      if (node.properties.length === 0) {
        return
      }

      const openBrace = sourceCode.getFirstToken(node)
      const closeBrace = sourceCode.getLastToken(node)
      const firstProperty = node.properties[0]
      const lastProperty = node.properties[node.properties.length - 1]
      const firstPropertyToken = sourceCode.getFirstToken(firstProperty)

      if (firstPropertyToken.loc.start.line > openBrace.loc.end.line + 1) {
        context.report({
          loc: {
            start: openBrace.loc.end,
            end: firstPropertyToken.loc.start
          },
          messageId: 'noBlankAfterOpen',
          fix(fixer) {
            return fixer.replaceTextRange(
              [openBrace.range[1], firstPropertyToken.range[0]],
              '\n' + ' '.repeat(firstPropertyToken.loc.start.column)
            )
          }
        })
      }

      for (let i = 0; i < node.properties.length - 1; i++) {
        const currentProp = node.properties[i]
        const nextProp = node.properties[i + 1]

        const currentLastToken = sourceCode.getLastToken(currentProp)
        const nextFirstToken = sourceCode.getFirstToken(nextProp)
        const tokenAfterCurrent = sourceCode.getTokenAfter(currentLastToken)
        const hasComma = tokenAfterCurrent && tokenAfterCurrent.value === ','
        const endToken = hasComma ? tokenAfterCurrent : currentLastToken

        if (nextFirstToken.loc.start.line > endToken.loc.end.line + 1) {
          context.report({
            loc: {
              start: endToken.loc.end,
              end: nextFirstToken.loc.start
            },
            messageId: 'noBlankBetweenProperties',
            fix(fixer) {
              return fixer.replaceTextRange(
                [endToken.range[1], nextFirstToken.range[0]],
                '\n' + ' '.repeat(nextFirstToken.loc.start.column)
              )
            }
          })
        }
      }

      const lastPropertyToken = sourceCode.getLastToken(lastProperty)

      if (closeBrace.loc.start.line > lastPropertyToken.loc.end.line + 1) {
        context.report({
          loc: {
            start: lastPropertyToken.loc.end,
            end: closeBrace.loc.start
          },
          messageId: 'noBlankBeforeClose',
          fix(fixer) {
            return fixer.replaceTextRange(
              [lastPropertyToken.range[1], closeBrace.range[0]],
              '\n' + ' '.repeat(closeBrace.loc.start.column)
            )
          }
        })
      }
    }

    return {
      ObjectExpression: checkObject
    }
  }
}