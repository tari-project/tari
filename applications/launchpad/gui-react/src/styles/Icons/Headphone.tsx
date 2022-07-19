import * as React from 'react'
import { SVGProps } from 'react'

const SvgHeadphone = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-headphone'
    {...props}
  >
    <path
      d='M2 17.02v-3.978l.024-.364c.314-4.68 3.648-8.566 8.142-9.491a9.093 9.093 0 0 1 3.668 0c4.494.925 7.828 4.812 8.142 9.49l.024.365v3.979m-6.41.78a7.228 7.228 0 0 1 0-2.697c.272-1.432 1.308-2.586 2.68-2.983l.29-.084c.303-.088.616-.132.93-.132 1.386 0 2.51 1.15 2.51 2.568v3.959C22 19.85 20.876 21 19.49 21c-.314 0-.627-.044-.93-.132l-.29-.084c-1.372-.398-2.408-1.55-2.68-2.983Zm-7.18 0a7.23 7.23 0 0 0 0-2.697c-.272-1.432-1.308-2.586-2.68-2.983l-.29-.084a3.34 3.34 0 0 0-.93-.132c-1.386 0-2.51 1.15-2.51 2.568v3.959C2 19.85 3.124 21 4.51 21c.314 0 .627-.044.93-.132l.29-.084c1.372-.398 2.408-1.55 2.68-2.983Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgHeadphone
