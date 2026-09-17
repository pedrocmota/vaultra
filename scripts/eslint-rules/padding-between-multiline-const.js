module.exports = {
  meta: {
    type: 'layout',
    fixable: 'whitespace',
    messages: {
      needBlank: 'Expected a blank line after a multiline const/let declaration when followed by another const/let.'
    }
  },
  create(context) {
    const sourceCode = context.getSourceCode()

    const isConstOrLet = (node) =>
      node &&
      node.type === 'VariableDeclaration' &&
      (node.kind === 'const' || node.kind === 'let')

    const isMultiline = (node) => node.loc.start.line !== node.loc.end.line

    const hasBlankLineBetween = (a, b) => {
      const lastA = sourceCode.getLastToken(a)
      const firstB = sourceCode.getFirstToken(b)

      if (!lastA || !firstB) {
        return false
      }

      const between = sourceCode.text.slice(lastA.range[1], firstB.range[0])

      return /\n\s*\n/.test(between)
    }

    const checkBody = (body) => {
      for (let i = 0; i < body.length - 1; i++) {
        const cur = body[i]
        const next = body[i + 1]

        if (!isConstOrLet(cur) || !isConstOrLet(next)) {
          continue
        }

        if (!isMultiline(cur)) {
          continue
        }

        if (hasBlankLineBetween(cur, next)) {
          continue
        }

        context.report({
          node: next,
          messageId: 'needBlank',
          fix(fixer) {
            return fixer.insertTextBefore(next, '\n')
          }
        })
      }
    }

    return {
      Program(node) {
        checkBody(node.body)
      },
      BlockStatement(node) {
        checkBody(node.body)
      }
    }
  }
}