module.exports = {
  meta: {
    type: 'suggestion',
    fixable: 'code',
    messages: {
      missingPublic: 'Missing accessibility modifier on {{ type }}. Adding "public".'
    }
  },
  create(context) {
    const sourceCode = context.getSourceCode()

    const hasAccessibilityModifier = (node) => {
      return node.accessibility === 'public' ||
             node.accessibility === 'private' ||
             node.accessibility === 'protected'
    }

    const checkNode = (node, type) => {
      if (node.kind === 'constructor') {
        return
      }

      if (hasAccessibilityModifier(node)) {
        return
      }

      context.report({
        node,
        messageId: 'missingPublic',
        data: { type },
        fix(fixer) {
          const firstToken = sourceCode.getFirstToken(node)
          let insertBefore = firstToken

          if (
            firstToken.value === 'static' ||
            firstToken.value === 'async' ||
            firstToken.value === 'readonly' ||
            firstToken.value === 'abstract' ||
            firstToken.value === 'override'
          ) {
            insertBefore = firstToken
          }

          return fixer.insertTextBefore(insertBefore, 'public ')
        }
      })
    }

    return {
      MethodDefinition(node) {
        checkNode(node, 'method')
      },
      PropertyDefinition(node) {
        checkNode(node, 'property')
      }
    }
  }
}