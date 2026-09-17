module.exports = {
  meta: {
    type: 'layout',
    fixable: 'whitespace',
    messages: {
      needBlank: 'Expected a blank line before this class member.'
    }
  },
  create(context) {
    const sourceCode = context.getSourceCode()

    const isPropertyDefinition = (node) => {
      return node.type === 'PropertyDefinition' || node.type === 'ClassProperty'
    }

    const hasBlankLineBetween = (a, b) => {
      const lastToken = sourceCode.getLastToken(a)
      const firstToken = sourceCode.getFirstToken(b)

      if (!lastToken || !firstToken) {
        return true
      }

      const textBetween = sourceCode.text.slice(lastToken.range[1], firstToken.range[0])

      return /\n\s*\n/.test(textBetween)
    }

    return {
      ClassBody(node) {
        const members = node.body

        for (let i = 1; i < members.length; i++) {
          const prev = members[i - 1]
          const curr = members[i]

          if (isPropertyDefinition(prev) && isPropertyDefinition(curr)) {
            continue
          }

          if (!hasBlankLineBetween(prev, curr)) {
            context.report({
              node: curr,
              messageId: 'needBlank',
              fix(fixer) {
                const lastToken = sourceCode.getLastToken(prev)

                return fixer.insertTextAfter(lastToken, '\n')
              }
            })
          }
        }
      }
    }
  }
}