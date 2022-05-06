import Box from '../Box'
import Tag from '../Tag'
import Text from '../Text'

import { BoxHeader, BoxContent, NodeBoxPlacholder } from './styles'
import { NodeBoxContentPlaceholderProps, NodeBoxProps } from './types'

/**
 * The advanced Box component handling:
 * - custom title
 * - header tag
 * - background depending on the status prop
 *
 * Used for the UI representation of the Node (Docker container) as a Box component.
 *
 * @param {string} [title] - the box heading
 * @param {{ text: string; type?: TagType }} [tag = 'inactive'] - the status of the box/node
 * @param {CSSWithSpring} [style] - the box style
 * @param {CSSWithSpring} [titleStyle] - the title style
 * @param {CSSWithSpring} [contentStyle] - the content style
 * @param {ReactNode} [children] - the box heading
 */
const NodeBox = ({
  title,
  tag,
  style,
  titleStyle,
  contentStyle,
  children,
  testId = 'node-box-cmp',
}: NodeBoxProps) => {
  return (
    <Box testId={testId} style={style}>
      <BoxHeader>
        {tag ? (
          <Tag type={tag.type} variant='large'>
            {tag.text}
          </Tag>
        ) : null}
      </BoxHeader>
      {title ? (
        <Text as='h2' type='header' style={titleStyle}>
          {title}
        </Text>
      ) : null}
      <BoxContent style={contentStyle}>{children}</BoxContent>
    </Box>
  )
}

/**
 * Simple placholder container for the node box that provides default spacing and layout.
 * @param {string | ReactNode} children - the content
 */
export const NodeBoxContentPlaceholder = ({
  children,
  testId = 'node-box-content-placeholder',
}: NodeBoxContentPlaceholderProps) => {
  let content = children

  if (typeof children === 'string') {
    content = <Text color='inherit'>{children}</Text>
  }

  return <NodeBoxPlacholder data-testid={testId}>{content}</NodeBoxPlacholder>
}

export default NodeBox
