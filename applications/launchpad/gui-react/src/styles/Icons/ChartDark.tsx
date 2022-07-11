import * as React from 'react'
import { SVGProps } from 'react'

const SvgChartDark = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 66 44'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-chartdark'
    {...props}
  >
    <rect
      x={0.811}
      width={65.189}
      height={44}
      rx={8}
      fill='#000'
      fillOpacity={0.4}
    />
    <path
      d='M25.071 19.74 10.518 34.293c-.63.63-.184 1.707.707 1.707H57a1 1 0 0 0 1-1V8L47.176 20.266a1 1 0 0 1-1.228.217l-7.06-3.85a1 1 0 0 0-1.078.077l-5.172 3.879a1 1 0 0 1-.817.176l-5.826-1.295a1 1 0 0 0-.924.27Z'
      fill='url(#ChartDark_svg__a)'
      fillOpacity={0.5}
    />
    <path
      d='m8.81 36 15.873-15.872a2 2 0 0 1 1.848-.538l4.841 1.075a2 2 0 0 0 1.634-.352l4.29-3.217a2 2 0 0 1 2.157-.156l5.794 3.16a2 2 0 0 0 2.458-.432L58 8'
      stroke='#5F9C91'
      strokeLinecap='round'
    />
    <defs>
      <linearGradient
        id='ChartDark_svg__a'
        x1={33.405}
        y1={8}
        x2={33.405}
        y2={36}
        gradientUnits='userSpaceOnUse'
      >
        <stop stopColor='#76A59D' />
        <stop offset={1} stopColor='#094E41' stopOpacity={0} />
      </linearGradient>
    </defs>
  </svg>
)

export default SvgChartDark
