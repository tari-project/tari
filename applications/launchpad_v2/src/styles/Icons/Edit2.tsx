import * as React from 'react'
import { SVGProps } from 'react'

const SvgEdit2 = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-edit2'
    {...props}
  >
    <path
      d='M17.25 10.992c-2.121.707-4.95-2.121-4.242-4.242m.871-.871-4.57 4.57a15.501 15.501 0 0 0-4.077 7.2l-.22.884a.376.376 0 0 0 .455.455l.883-.22a15.501 15.501 0 0 0 7.202-4.078l4.57-4.57a3 3 0 1 0-4.243-4.241Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgEdit2
