import * as React from 'react'
import { SVGProps } from 'react'

const SvgReserve = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-reserve'
    {...props}
  >
    <path
      d='M2 19.25a.75.75 0 0 0 0 1.5v-1.5Zm20 1.5a.75.75 0 0 0 0-1.5v1.5ZM4.972 20c0-3.831 3.14-6.95 7.028-6.95v-1.5c-4.703 0-8.528 3.776-8.528 8.45h1.5ZM12 13.05c3.889 0 7.028 3.119 7.028 6.95h1.5c0-4.674-3.825-8.45-8.528-8.45v1.5Zm-2.028-1.575c0-.41.218-.835.622-1.179.406-.345.932-.546 1.406-.546v-1.5c-.866 0-1.73.353-2.377.903-.65.553-1.15 1.365-1.15 2.322h1.5ZM12 9.75c.474 0 1 .201 1.406.546.404.344.622.769.622 1.179h1.5c0-.957-.501-1.77-1.15-2.322-.649-.55-1.512-.903-2.378-.903v1.5Zm2.028 1.725v1.375h1.5v-1.375h-1.5Zm-5.556 0v1.375h1.5v-1.375h-1.5ZM2 20.75h20v-1.5H2v1.5Z'
      fill='currentColor'
    />
    <path
      d='M12 5V4m4.5 2.062L15.562 7M8.438 7 7.5 6.062'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgReserve
