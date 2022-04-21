import * as React from 'react'
import { SVGProps } from 'react'

const SvgText = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    {...props}
  >
    <path
      d='m3.353 15.05.73-.172-.73.172Zm0-6.1.73.172-.73-.172Zm17.294 0-.73.172.73-.172Zm0 6.1.73.17-.73-.17Zm-5.597 5.597-.172-.73.172.73Zm-6.1 0-.17.73.17-.73Zm0-17.294.172.73-.172-.73Zm6.1 0-.172.73.172-.73Zm-3.8 11.768a.75.75 0 0 0 1.5 0h-1.5ZM8.88 8.129a.75.75 0 0 0 0 1.5v-1.5Zm6.24 1.5a.75.75 0 0 0 0-1.5v1.5ZM4.084 14.88a12.604 12.604 0 0 1 0-5.757l-1.46-.343a14.103 14.103 0 0 0 0 6.442l1.46-.343Zm15.834-5.757a12.603 12.603 0 0 1 0 5.756l1.46.343a14.104 14.104 0 0 0 0-6.442l-1.46.343Zm-5.039 10.795a12.603 12.603 0 0 1-5.756 0l-.343 1.46c2.119.497 4.323.497 6.442 0l-.343-1.46ZM9.122 4.083a12.604 12.604 0 0 1 5.756 0l.343-1.46a14.103 14.103 0 0 0-6.442 0l.343 1.46Zm0 15.834a6.761 6.761 0 0 1-5.039-5.039l-1.46.343a8.261 8.261 0 0 0 6.156 6.156l.343-1.46Zm6.099 1.46a8.261 8.261 0 0 0 6.156-6.156l-1.46-.343a6.761 6.761 0 0 1-5.039 5.039l.343 1.46Zm-.343-17.294a6.761 6.761 0 0 1 5.039 5.039l1.46-.343a8.261 8.261 0 0 0-6.156-6.156l-.343 1.46ZM8.78 2.623a8.261 8.261 0 0 0-6.156 6.156l1.46.343a6.761 6.761 0 0 1 5.039-5.039l-.343-1.46Zm2.471 6.256v6.242h1.5V8.879h-1.5Zm.75-.75H8.88v1.5H12v-1.5Zm0 1.5h3.12v-1.5H12v1.5Z'
      fill='currentColor'
    />
  </svg>
)

export default SvgText
