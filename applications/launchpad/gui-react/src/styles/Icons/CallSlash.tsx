import * as React from 'react'
import { SVGProps } from 'react'

const SvgCallSlash = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-callslash'
    {...props}
  >
    <path
      d='M3.116 8.835a7.704 7.704 0 0 1 0-2.67c.25-1.417 1.201-2.558 2.46-2.951l.267-.083A2.86 2.86 0 0 1 6.697 3C7.969 3 9 4.138 9 5.541V9.46C9 10.862 7.969 12 6.697 12c-.289 0-.576-.044-.854-.13l-.267-.084c-1.259-.393-2.21-1.534-2.46-2.951Zm0 0a15.391 15.391 0 0 0 3.58 7.138'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='M18.53 5.53a.75.75 0 0 0-1.06-1.06l1.06 1.06ZM2.47 19.47a.75.75 0 1 0 1.06 1.06l-1.06-1.06Zm15-15-15 15 1.06 1.06 15-15-1.06-1.06Z'
      fill='currentColor'
    />
    <path
      d='M15.165 20.884a7.705 7.705 0 0 0 2.67 0c1.416-.25 2.558-1.201 2.95-2.46l.084-.267c.087-.278.13-.565.13-.854C21 16.031 19.863 15 18.46 15H14.54C13.138 15 12 16.031 12 17.303c0 .289.044.576.13.854l.084.267c.393 1.259 1.534 2.21 2.951 2.46Zm0 0a15.015 15.015 0 0 1-6.245-2.727'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgCallSlash
