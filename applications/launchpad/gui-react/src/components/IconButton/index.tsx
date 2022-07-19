import Button from '../Button'
import { ButtonProps } from '../Button/types'

const IconButton = ({
  style,
  children,
  ...props
}: Omit<ButtonProps, 'variant'>) => {
  return (
    <Button
      variant='text'
      {...props}
      style={{
        ...style,
        padding: 0,
        display: 'inline-block',
      }}
    >
      {children}
    </Button>
  )
}

export default IconButton
