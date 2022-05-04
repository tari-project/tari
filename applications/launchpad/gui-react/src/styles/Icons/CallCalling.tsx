import * as React from 'react'
import { SVGProps } from 'react'

const SvgCallCalling = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-callcalling'
    {...props}
  >
    <path
      d='M13 3.252c2.163-.616 4.543-.105 6.198 1.55 1.655 1.655 2.166 4.035 1.55 6.198m-5.811-5.036c.772-.154 1.662.113 2.324.775.662.662.929 1.552.775 2.324'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
    <path
      d='M15.165 20.884a7.705 7.705 0 0 0 2.67 0c1.417-.25 2.558-1.201 2.951-2.46l.083-.267c.087-.278.131-.565.131-.854C21 16.031 19.862 15 18.459 15H14.54C13.138 15 12 16.031 12 17.303c0 .289.044.576.13.854l.084.267c.393 1.259 1.534 2.21 2.951 2.46Zm0 0A15.04 15.04 0 0 1 3.117 8.834m0 0a7.704 7.704 0 0 1 0-2.669c.25-1.417 1.2-2.558 2.46-2.951l.266-.083A2.86 2.86 0 0 1 6.697 3C7.969 3 9 4.138 9 5.541V9.46C9 10.862 7.969 12 6.697 12c-.289 0-.576-.044-.854-.13l-.267-.084c-1.259-.393-2.21-1.534-2.46-2.951Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgCallCalling
