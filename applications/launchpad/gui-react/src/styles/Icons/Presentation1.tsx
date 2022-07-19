import * as React from 'react'
import { SVGProps } from 'react'

const SvgPresentation1 = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-presentation1'
    {...props}
  >
    <path
      d='M2.42 8.72c.691-2.65 2.802-4.676 5.452-5.235l.507-.107a17.537 17.537 0 0 1 7.242 0l.507.107c2.65.559 4.76 2.586 5.451 5.234.561 2.15.561 4.411 0 6.562-.69 2.649-2.8 4.675-5.451 5.234l-.507.107a17.536 17.536 0 0 1-7.242 0l-.507-.107c-2.65-.559-4.76-2.585-5.451-5.234-.561-2.15-.561-4.411 0-6.562Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='M6 12h2l2 3 4-6 2 3h2'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgPresentation1
